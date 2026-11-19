//! Speech boundary detector mirroring Android `SpeechSegmenter.java`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    None,
    Start,
    End,
}

#[derive(Debug, Default)]
pub struct SpeechSegmenter {
    onset: u32,
    silence: u32,
    active: bool,
}

impl SpeechSegmenter {
    pub const ONSET_THRESHOLD: f32 = 0.50;
    pub const SILENCE_THRESHOLD: f32 = 0.35;
    pub const ONSET_FRAMES: u32 = 3;
    pub const HANGOVER_FRAMES: u32 = 32;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn boundary(&mut self, probability: f32) -> anyhow::Result<Boundary> {
        if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
            anyhow::bail!("vad_probability");
        }
        if !self.active {
            if probability >= Self::ONSET_THRESHOLD {
                self.onset += 1;
            } else {
                self.onset = 0;
            }
            if self.onset == Self::ONSET_FRAMES {
                self.active = true;
                self.silence = 0;
                return Ok(Boundary::Start);
            }
        } else {
            if probability < Self::SILENCE_THRESHOLD {
                self.silence += 1;
            } else {
                self.silence = 0;
            }
            if self.silence >= Self::HANGOVER_FRAMES {
                self.reset();
                return Ok(Boundary::End);
            }
        }
        Ok(Boundary::None)
    }

    pub fn reset(&mut self) {
        self.onset = 0;
        self.silence = 0;
        self.active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onset_requires_three_frames_at_or_above_half() {
        let mut seg = SpeechSegmenter::new();
        assert_eq!(seg.boundary(0.49).unwrap(), Boundary::None);
        assert_eq!(seg.boundary(0.50).unwrap(), Boundary::None);
        assert_eq!(seg.boundary(0.65).unwrap(), Boundary::None);
        assert_eq!(seg.boundary(0.51).unwrap(), Boundary::Start);
        assert!(seg.is_active());
    }

    #[test]
    fn hangover_ends_after_32_quiet_frames() {
        let mut seg = SpeechSegmenter::new();
        for _ in 0..3 {
            seg.boundary(0.8).unwrap();
        }
        for _ in 0..31 {
            assert_eq!(seg.boundary(0.2).unwrap(), Boundary::None);
        }
        assert_eq!(seg.boundary(0.1).unwrap(), Boundary::End);
        assert!(!seg.is_active());
    }

    #[test]
    fn rejects_invalid_probabilities() {
        let mut seg = SpeechSegmenter::new();
        assert!(seg.boundary(f32::NAN).is_err());
        assert!(seg.boundary(1.2).is_err());
        assert!(seg.boundary(-0.1).is_err());
    }
}
