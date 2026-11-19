//! Audio capture via `cpal`.
//!
//! Ported from `android/.../audio/core/LocalCapture.java` and
//! `CapturePump.java`. Captures 16 kHz mono PCM16 from the default input
//! device and hands frames to a callback. On Windows this uses WASAPI
//! (shared mode); exclusive-mode low-latency capture is a follow-up.
//!
//! The capture loop is intentionally decoupled from the WS/upload layer: the
//! caller owns the [`crate::half_duplex::HalfDuplexGate`] and the upload
//! buffer, and this module only pulls raw PCM out of the device.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, SizedSample};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Capture configuration. Defaults match the Android voice path: 16 kHz mono
/// PCM16.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            channels: 1,
        }
    }
}

/// A running capture session. Drop it (or call `stop()`) to release the
/// device. Frames are delivered to `on_frame` on the audio thread.
pub struct CaptureSession {
    stop: Arc<AtomicBool>,
    // `cpal::Stream` is not `Send` on all platforms; keep it alongside the
    // flag so dropping the session stops the stream.
    _stream: cpal::Stream,
}

// cpal::Stream is !Send on some backends but we only hold it to keep it alive.
unsafe impl Send for CaptureSession {}

impl CaptureSession {
    /// Signal the capture loop to stop. The stream is also stopped on drop.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Start capturing from the default input device. `on_frame` is called with a
/// buffer of `i16` samples (interleaved if `channels > 1`; for mono this is a
/// single channel). The callback runs on the audio thread — keep it short and
/// non-blocking.
pub fn start_capture<F>(config: &CaptureConfig, on_frame: F) -> Result<CaptureSession>
where
    F: FnMut(Vec<i16>) + Send + 'static,
{
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device"))?;

    // Find a supported config range that covers our requested sample rate and
    // channel count; prefer the device's native sample format.
    let supported_ranges: Vec<_> = device
        .supported_input_configs()
        .context("query supported input configs")?
        .collect();
    if supported_ranges.is_empty() {
        return Err(anyhow!("no supported input configs"));
    }

    let chosen_range = supported_ranges
        .iter()
        .filter(|c| c.channels() >= config.channels)
        .find(|c| {
            c.min_sample_rate().0 <= config.sample_rate
                && c.max_sample_rate().0 >= config.sample_rate
        })
        .or_else(|| {
            supported_ranges
                .iter()
                .find(|c| c.channels() >= config.channels)
        })
        .or_else(|| supported_ranges.first())
        .ok_or_else(|| anyhow!("no compatible input config"))?
        .clone();

    let sample_format = chosen_range.sample_format();
    let sample_rate = cpal::SampleRate(
        config
            .sample_rate
            .clamp(chosen_range.min_sample_rate().0, chosen_range.max_sample_rate().0),
    );
    let channels = config.channels.min(chosen_range.channels());
    let stream_config = cpal::StreamConfig {
        channels,
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();

    let stream = match sample_format {
        SampleFormat::F32 => {
            build_stream::<f32, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::I16 => {
            build_stream::<i16, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::U16 => {
            build_stream::<u16, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::I8 => {
            build_stream::<i8, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::U8 => {
            build_stream::<u8, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::I32 => {
            build_stream::<i32, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::I64 => {
            build_stream::<i64, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::U32 => {
            build_stream::<u32, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::U64 => {
            build_stream::<u64, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        SampleFormat::F64 => {
            build_stream::<f64, _>(&device, &stream_config, stop_clone, on_frame)?
        }
        other => return Err(anyhow!("unsupported sample format: {other:?}")),
    };

    stream.play().context("start capture stream")?;
    Ok(CaptureSession {
        stop,
        _stream: stream,
    })
}

fn build_stream<T, F>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
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

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            // Convert device samples -> f32 -> i16 PCM.
            let pcm: Vec<i16> = data
                .iter()
                .map(|&s| {
                    let f: f32 = cpal::Sample::from_sample(s);
                    let clamped = f.clamp(-1.0, 1.0);
                    (clamped * 32767.0) as i16
                })
                .collect();
            on_frame(pcm);
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}

/// Convert interleaved i16 PCM bytes (little-endian) into f32 samples in [-1, 1].
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

/// Convert f32 samples in [-1, 1] to interleaved i16 PCM bytes (little-endian).
pub fn f32_to_i16_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

// ── Tests ───────────────────────────────────────────────────────────────

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
    fn stop_flag_is_observable() {
        let stop = Arc::new(AtomicBool::new(false));
        assert!(!stop.load(Ordering::SeqCst));
        stop.store(true, Ordering::SeqCst);
        assert!(stop.load(Ordering::SeqCst));
    }

    #[test]
    fn i16_bytes_roundtrip_shape() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let bytes = f32_to_i16_bytes(&samples);
        assert_eq!(bytes.len(), samples.len() * 2);
        let back = i16_bytes_to_f32(&bytes);
        assert_eq!(back.len(), samples.len());
        // Round-trip is lossy at the LSB; values should be close.
        for (a, b) in samples.iter().zip(back.iter()) {
            assert!((a - b).abs() < 0.002, "roundtrip drift: {a} vs {b}");
        }
    }
}
