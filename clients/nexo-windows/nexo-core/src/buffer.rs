//! Pre-roll ring + bounded PCM16 utterance accumulator mirroring `UtteranceBuffer.java`.

use crate::upload::Sealed;

pub const MAX_SAMPLES: usize = 320_000; // 20 s @ 16 kHz
pub const PRE_ROLL_SAMPLES: usize = 6_400; // 400 ms @ 16 kHz

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturePurpose {
    Conversation,
    Enrollment,
    Diagnostic,
}

impl CapturePurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::Enrollment => "enrollment",
            Self::Diagnostic => "diagnostic",
        }
    }
}

#[derive(Debug)]
pub struct UtteranceBuffer {
    max_samples: usize,
    pre: Vec<i16>,
    pre_write: usize,
    pre_size: usize,
    pcm: Vec<i16>,
    capturing: bool,
    purpose: Option<CapturePurpose>,
    failure: Option<&'static str>,
    samples: u64,
    clipped: u64,
    squares: f64,
    peak: i32,
}

impl Default for UtteranceBuffer {
    fn default() -> Self {
        Self::new(MAX_SAMPLES, PRE_ROLL_SAMPLES)
    }
}

impl UtteranceBuffer {
    pub fn new(max_samples: usize, pre_roll_samples: usize) -> Self {
        assert!(max_samples >= 1 && max_samples <= MAX_SAMPLES);
        assert!(pre_roll_samples < max_samples);
        Self {
            max_samples,
            pre: vec![0i16; pre_roll_samples],
            pre_write: 0,
            pre_size: 0,
            pcm: Vec::with_capacity(max_samples),
            capturing: false,
            purpose: None,
            failure: None,
            samples: 0,
            clipped: 0,
            squares: 0.0,
            peak: 0,
        }
    }

    /// Feed mono 16 kHz samples. Returns a sealed utterance if `duration_limit` is reached.
    pub fn accept(&mut self, data: &[i16], now_ms: i64) -> Option<Sealed> {
        if self.failure.is_some() || data.is_empty() {
            return None;
        }
        if !self.capturing && self.samples >= 8_000 {
            self.squares *= 0.5;
            self.samples /= 2;
        }
        for &value in data {
            self.record_metric(value);
            if self.capturing {
                self.pcm.push(value);
                if self.pcm.len() >= self.max_samples {
                    return self.finish("duration_limit", now_ms);
                }
            } else if !self.pre.is_empty() {
                if self.pre_size < self.pre.len() {
                    self.pre_size += 1;
                }
                self.pre[self.pre_write] = value;
                self.pre_write = (self.pre_write + 1) % self.pre.len();
            }
        }
        None
    }

    pub fn start(&mut self, requested_purpose: CapturePurpose) -> bool {
        if self.capturing || self.failure.is_some() {
            return false;
        }
        self.purpose = Some(requested_purpose);
        self.pcm.clear();
        self.samples = 0;
        self.clipped = 0;
        self.squares = 0.0;
        self.peak = 0;

        if !self.pre.is_empty() && self.pre_size > 0 {
            let oldest = (self.pre_write + self.pre.len() - self.pre_size) % self.pre.len();
            for i in 0..self.pre_size {
                let sample = self.pre[(oldest + i) % self.pre.len()];
                self.record_metric(sample);
                self.pcm.push(sample);
            }
            self.pre.fill(0);
            self.pre_size = 0;
            self.pre_write = 0;
        }

        self.capturing = true;
        true
    }

    pub fn finish(&mut self, reason: &str, now_ms: i64) -> Option<Sealed> {
        if !self.capturing {
            return None;
        }
        if self.pcm.is_empty() {
            self.invalidate("no_audio");
            return None;
        }
        if !matches!(reason, "manual_finish" | "vad_end" | "duration_limit") {
            self.invalidate("invalid_reason");
            return None;
        }
        let purpose = self.purpose?.as_str();
        self.capturing = false;
        let mut bytes = Vec::with_capacity(self.pcm.len() * 2);
        for &s in &self.pcm {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.pcm.fill(0);
        self.pcm.clear();
        Some(Sealed::new(bytes, purpose, reason, now_ms))
    }

    pub fn invalidate(&mut self, code: &'static str) {
        self.cancel();
        self.failure = Some(code);
    }

    pub fn cancel(&mut self) {
        self.capturing = false;
        self.purpose = None;
        self.failure = None;
        self.pcm.fill(0);
        self.pcm.clear();
        self.pre.fill(0);
        self.pre_size = 0;
        self.pre_write = 0;
        self.samples = 0;
        self.clipped = 0;
        self.squares = 0.0;
        self.peak = 0;
    }

    pub fn is_capturing(&self) -> bool {
        self.capturing
    }

    pub fn error(&self) -> Option<&'static str> {
        self.failure
    }

    pub fn rms(&self) -> f32 {
        if self.samples == 0 {
            0.0
        } else {
            ((self.squares / self.samples as f64).sqrt() / 32768.0).clamp(0.0, 1.0) as f32
        }
    }

    pub fn peak(&self) -> i32 {
        self.peak
    }

    pub fn clipped_samples(&self) -> u64 {
        self.clipped
    }

    fn record_metric(&mut self, value: i16) {
        self.samples += 1;
        let v = value as f64;
        self.squares += v * v;
        let abs = (value as i32).abs();
        if abs > self.peak {
            self.peak = abs;
        }
        if value == i16::MIN || value == i16::MAX {
            self.clipped += 1;
        }
    }
}

/// Apply bounded adaptive gain (up to 6x for soft voice) on a 512-sample VAD window
/// without mutating raw PCM stored for STT. Mirrors `LocalCapture.java`.
pub fn prepare_vad_window(raw_i16: &[i16], out: &mut [f32; 512]) {
    let mut peak = 0.0f32;
    for (i, &s) in raw_i16.iter().take(512).enumerate() {
        let f = s as f32 / 32768.0;
        out[i] = f;
        peak = peak.max(f.abs());
    }
    let gain = if (0.008..0.35).contains(&peak) {
        (0.35 / peak).min(6.0)
    } else {
        1.0
    };
    if (gain - 1.0).abs() > f32::EPSILON {
        for sample in out.iter_mut() {
            *sample = (*sample * gain).clamp(-1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_roll_and_finish_produce_valid_sealed_pcm() {
        let mut buf = UtteranceBuffer::new(1000, 100);
        buf.accept(&vec![500i16; 150], 1);
        assert!(buf.start(CapturePurpose::Conversation));
        buf.accept(&vec![1000i16; 200], 2);
        let sealed = buf.finish("vad_end", 3).expect("should seal");
        assert_eq!(sealed.samples(), 300); // 100 pre-roll + 200 live
        assert_eq!(sealed.purpose, "conversation");
        assert_eq!(sealed.end_reason, "vad_end");
    }

    #[test]
    fn duration_limit_auto_seals() {
        let mut buf = UtteranceBuffer::new(256, 16);
        assert!(buf.start(CapturePurpose::Conversation));
        let sealed = buf.accept(&vec![42i16; 256], 10).expect("auto seal");
        assert_eq!(sealed.samples(), 256);
        assert_eq!(sealed.end_reason, "duration_limit");
    }
}
