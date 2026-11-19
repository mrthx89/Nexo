//! Audio capture & playback via `cpal` (WASAPI on Windows).
//!
//! Mirrors `AudioRecorderManager.kt` + `AudioPlayerManager.kt`:
//! - Capture: captures from the default input device, downmixes to mono and
//!   linear-resamples to 16 kHz PCM16 for Silero VAD + STT.
//! - Playback: streams 24 kHz mono PCM16 server TTS chunks into a bounded
//!   output ring buffer on the default output device, resampling to the
//!   device's native rate/channels with drain signaling.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, SizedSample};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub const CAPTURE_SAMPLE_RATE: u32 = 16_000;
pub const PLAYBACK_SAMPLE_RATE: u32 = 24_000;
/// Bounded output queue (~1.5s at 24 kHz) matching Android's backpressure cap.
const PLAYBACK_QUEUE_CAP_SAMPLES: usize = 36_000;

#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: CAPTURE_SAMPLE_RATE,
            channels: 1,
        }
    }
}

pub struct CaptureSession {
    stop: Arc<AtomicBool>,
    _stream: cpal::Stream,
}

unsafe impl Send for CaptureSession {}

impl CaptureSession {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Start capturing from the default input device, delivering 16 kHz mono `i16` frames.
pub fn start_capture<F>(config: &CaptureConfig, on_frame: F) -> Result<CaptureSession>
where
    F: FnMut(Vec<i16>) + Send + 'static,
{
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device"))?;

    let supported_ranges: Vec<_> = device
        .supported_input_configs()
        .context("query supported input configs")?
        .collect();
    if supported_ranges.is_empty() {
        return Err(anyhow!("no supported input configs"));
    }

    let chosen_range = supported_ranges
        .iter()
        .find(|c| {
            c.min_sample_rate().0 <= config.sample_rate
                && c.max_sample_rate().0 >= config.sample_rate
        })
        .or_else(|| supported_ranges.first())
        .ok_or_else(|| anyhow!("no compatible input config"))?
        .clone();

    let sample_format = chosen_range.sample_format();
    let device_rate = config
        .sample_rate
        .clamp(chosen_range.min_sample_rate().0, chosen_range.max_sample_rate().0);
    let device_channels = chosen_range.channels().max(1);
    let stream_config = cpal::StreamConfig {
        channels: device_channels,
        sample_rate: cpal::SampleRate(device_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();

    let stream = match sample_format {
        SampleFormat::F32 => build_input_stream::<f32, _>(
            &device,
            &stream_config,
            device_rate,
            device_channels,
            config.sample_rate,
            stop_clone,
            on_frame,
        )?,
        SampleFormat::I16 => build_input_stream::<i16, _>(
            &device,
            &stream_config,
            device_rate,
            device_channels,
            config.sample_rate,
            stop_clone,
            on_frame,
        )?,
        SampleFormat::U16 => build_input_stream::<u16, _>(
            &device,
            &stream_config,
            device_rate,
            device_channels,
            config.sample_rate,
            stop_clone,
            on_frame,
        )?,
        SampleFormat::I32 => build_input_stream::<i32, _>(
            &device,
            &stream_config,
            device_rate,
            device_channels,
            config.sample_rate,
            stop_clone,
            on_frame,
        )?,
        other => return Err(anyhow!("unsupported capture sample format: {other:?}")),
    };

    stream.play().context("start capture stream")?;
    Ok(CaptureSession {
        stop,
        _stream: stream,
    })
}

fn build_input_stream<T, F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    in_rate: u32,
    in_channels: u16,
    out_rate: u32,
    stop: Arc<AtomicBool>,
    mut on_frame: F,
) -> Result<cpal::Stream>
where
    T: SizedSample + Sample + Send + 'static,
    F: FnMut(Vec<i16>) + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let err_fn = |err| {
        tracing::error!(%err, "capture stream error");
    };
    let mut resampler = LinearResampler::new(in_rate, out_rate);

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            let ch = in_channels as usize;
            let frames = data.len() / ch;
            let mut mono = Vec::with_capacity(frames);
            for frame_idx in 0..frames {
                let mut sum = 0.0f32;
                for c in 0..ch {
                    sum += f32::from_sample(data[frame_idx * ch + c]);
                }
                mono.push((sum / ch as f32).clamp(-1.0, 1.0));
            }
            let out: Vec<i16> = if in_rate == out_rate {
                mono.into_iter()
                    .map(|f| (f * 32767.0).clamp(-32768.0, 32767.0) as i16)
                    .collect()
            } else {
                resampler
                    .process(&mono)
                    .into_iter()
                    .map(|f| (f * 32767.0).clamp(-32768.0, 32767.0) as i16)
                    .collect()
            };
            if !out.is_empty() {
                on_frame(out);
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

#[derive(Debug)]
pub struct LinearResampler {
    in_rate: u32,
    out_rate: u32,
    pos: f64,
    last: f32,
}

impl LinearResampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        Self {
            in_rate: in_rate.max(1),
            out_rate: out_rate.max(1),
            pos: 0.0,
            last: 0.0,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        if self.in_rate == self.out_rate {
            self.last = *input.last().unwrap_or(&0.0);
            return input.to_vec();
        }
        let step = self.in_rate as f64 / self.out_rate as f64;
        let estimated = ((input.len() as f64) / step).ceil() as usize + 2;
        let mut out = Vec::with_capacity(estimated);
        while self.pos < input.len() as f64 {
            let idx = self.pos.floor() as isize;
            let frac = (self.pos - idx as f64) as f32;
            let s0 = if idx < 0 {
                self.last
            } else {
                input[idx as usize]
            };
            let s1 = if (idx + 1) as usize >= input.len() {
                input[input.len() - 1]
            } else if idx + 1 < 0 {
                self.last
            } else {
                input[(idx + 1) as usize]
            };
            out.push(s0 + (s1 - s0) * frac);
            self.pos += step;
        }
        self.pos -= input.len() as f64;
        self.last = *input.last().unwrap_or(&0.0);
        out
    }
}

struct PlaybackShared {
    queue: VecDeque<f32>,
    epoch: u64,
    closed: bool,
    draining: bool,
}

/// Bounded PCM16 24 kHz mono playback engine matching `AudioPlayerManager.kt`.
pub struct PlaybackSession {
    shared: Arc<(Mutex<PlaybackShared>, Condvar)>,
    _stream: cpal::Stream,
    resampler: Mutex<LinearResampler>,
}

unsafe impl Send for PlaybackSession {}
unsafe impl Sync for PlaybackSession {}

impl PlaybackSession {
    pub fn start_default() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no default output device"))?;
        let supported = device
            .supported_output_configs()
            .context("query supported output configs")?
            .collect::<Vec<_>>();
        let chosen = supported
            .iter()
            .find(|c| {
                c.min_sample_rate().0 <= PLAYBACK_SAMPLE_RATE
                    && c.max_sample_rate().0 >= PLAYBACK_SAMPLE_RATE
            })
            .or_else(|| supported.first())
            .ok_or_else(|| anyhow!("no compatible output config"))?
            .clone();

        let format = chosen.sample_format();
        let out_rate = PLAYBACK_SAMPLE_RATE
            .clamp(chosen.min_sample_rate().0, chosen.max_sample_rate().0);
        let out_channels = chosen.channels().max(1);
        let config = cpal::StreamConfig {
            channels: out_channels,
            sample_rate: cpal::SampleRate(out_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let shared = Arc::new((
            Mutex::new(PlaybackShared {
                queue: VecDeque::with_capacity(PLAYBACK_QUEUE_CAP_SAMPLES),
                epoch: 0,
                closed: false,
                draining: false,
            }),
            Condvar::new(),
        ));

        let shared_cb = Arc::clone(&shared);
        let stream = match format {
            SampleFormat::F32 => build_output_stream::<f32>(&device, &config, out_channels, shared_cb)?,
            SampleFormat::I16 => build_output_stream::<i16>(&device, &config, out_channels, shared_cb)?,
            SampleFormat::U16 => build_output_stream::<u16>(&device, &config, out_channels, shared_cb)?,
            other => return Err(anyhow!("unsupported playback sample format: {other:?}")),
        };
        stream.play().context("start playback stream")?;

        Ok(Self {
            shared,
            _stream: stream,
            resampler: Mutex::new(LinearResampler::new(PLAYBACK_SAMPLE_RATE, out_rate)),
        })
    }

    pub fn current_epoch(&self) -> u64 {
        self.shared.0.lock().unwrap().epoch
    }

    /// Clear queued samples and increment generation epoch so in-flight writes stop immediately.
    pub fn reset_epoch(&self) -> u64 {
        let (lock, cvar) = &*self.shared;
        let mut g = lock.lock().unwrap();
        g.epoch = g.epoch.wrapping_add(1);
        g.queue.clear();
        g.draining = false;
        cvar.notify_all();
        g.epoch
    }

    /// Push little-endian PCM16 mono samples (default 24 kHz; pass custom `src_rate` for 16 kHz diagnostic WAVs).
    pub fn write_pcm16_with_rate(&self, epoch: u64, pcm_bytes: &[u8], src_rate: u32) -> bool {
        let mut floats = Vec::with_capacity(pcm_bytes.len() / 2);
        let mut i = 0;
        while i + 1 < pcm_bytes.len() {
            let s = i16::from_le_bytes([pcm_bytes[i], pcm_bytes[i + 1]]);
            floats.push(s as f32 / 32768.0);
            i += 2;
        }
        let converted = if src_rate == PLAYBACK_SAMPLE_RATE {
            self.resampler.lock().unwrap().process(&floats)
        } else {
            // For ad-hoc diagnostic playback (16 kHz)
            let out_rate = self.resampler.lock().unwrap().out_rate;
            LinearResampler::new(src_rate, out_rate).process(&floats)
        };

        let (lock, cvar) = &*self.shared;
        let mut g = lock.lock().unwrap();
        for sample in converted {
            while g.queue.len() >= PLAYBACK_QUEUE_CAP_SAMPLES {
                if g.closed || g.epoch != epoch {
                    return false;
                }
                g = cvar.wait(g).unwrap();
            }
            if g.closed || g.epoch != epoch {
                return false;
            }
            g.queue.push_back(sample);
        }
        true
    }

    pub fn write_pcm16(&self, epoch: u64, pcm_bytes: &[u8]) -> bool {
        self.write_pcm16_with_rate(epoch, pcm_bytes, PLAYBACK_SAMPLE_RATE)
    }

    /// Wait until queued TTS samples drain (bounded to `timeout`) or epoch changes.
    pub fn wait_drained(&self, epoch: u64, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let (lock, cvar) = &*self.shared;
        let mut g = lock.lock().unwrap();
        g.draining = true;
        while !g.queue.is_empty() && !g.closed && g.epoch == epoch {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (guard, res) = cvar.wait_timeout(g, deadline - now).unwrap();
            g = guard;
            if res.timed_out() && !g.queue.is_empty() {
                return false;
            }
        }
        g.draining = false;
        g.epoch == epoch && !g.closed
    }
}

impl Drop for PlaybackSession {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.shared;
        let mut g = lock.lock().unwrap();
        g.closed = true;
        g.queue.clear();
        cvar.notify_all();
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: u16,
    shared: Arc<(Mutex<PlaybackShared>, Condvar)>,
) -> Result<cpal::Stream>
where
    T: SizedSample + Sample + cpal::FromSample<f32> + Send + 'static,
{
    let ch = channels as usize;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let (lock, cvar) = &*shared;
            let mut g = lock.lock().unwrap();
            let frames = data.len() / ch;
            for f in 0..frames {
                let val = g.queue.pop_front().unwrap_or(0.0);
                let typed = T::from_sample(val);
                for c in 0..ch {
                    data[f * ch + c] = typed;
                }
            }
            cvar.notify_all();
        },
        |err| tracing::error!(%err, "playback stream error"),
        None,
    )?;
    Ok(stream)
}

pub fn i16_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0;
    while i + 1 < bytes.len() {
        let sample = i16::from_le_bytes([bytes[i], bytes[i + 1]]);
        out.push(sample as f32 / 32768.0);
        i += 2;
    }
    out
}

pub fn f32_to_i16_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_config_defaults_match_android() {
        let cfg = CaptureConfig::default();
        assert_eq!(cfg.sample_rate, 16_000);
        assert_eq!(cfg.channels, 1);
    }

    #[test]
    fn linear_resampler_48k_to_16k_ratio() {
        let mut r = LinearResampler::new(48_000, 16_000);
        let input: Vec<f32> = (0..480).map(|i| (i as f32 / 480.0).sin()).collect();
        let out = r.process(&input);
        assert!((out.len() as isize - 160).abs() <= 1);
    }

    #[test]
    fn i16_bytes_roundtrip_shape() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let bytes = f32_to_i16_bytes(&samples);
        assert_eq!(bytes.len(), samples.len() * 2);
        let back = i16_bytes_to_f32(&bytes);
        assert_eq!(back.len(), samples.len());
        for (a, b) in samples.iter().zip(back.iter()) {
            assert!((a - b).abs() < 0.002, "roundtrip drift: {a} vs {b}");
        }
    }
}
