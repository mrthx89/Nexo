//! Private local microphone diagnostic clip mirroring `PrivateDiagnosticClip.java`.
//!
//! Only stores clips with explicit consent (`purpose == "diagnostic"`), never touched by upload,
//! auto-expires after 5 minutes, and deletes immediately on close/dismiss.

use crate::upload::Sealed;
use anyhow::Result;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const DIAGNOSTIC_TTL: Duration = Duration::from_secs(300);

pub struct PrivateDiagnosticClip {
    file_path: PathBuf,
    retained_at: Option<Instant>,
    pcm_bytes: usize,
}

impl PrivateDiagnosticClip {
    pub fn new(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let file_path = dir.join("local-test.wav");
        let mut clip = Self {
            file_path,
            retained_at: None,
            pcm_bytes: 0,
        };
        clip.delete()?;
        Ok(clip)
    }

    pub fn retain(&mut self, sealed: &mut Sealed, explicit_consent: bool) -> Result<bool> {
        if !explicit_consent || sealed.purpose != "diagnostic" || sealed.is_released() {
            return Ok(false);
        }
        self.delete()?;
        let total_samples = sealed.samples();
        let data_len = total_samples * 2;
        let mut out = File::create(&self.file_path)?;
        out.write_all(&wav_header(data_len, 16_000, 1))?;
        let mut offset = 0u32;
        while offset < total_samples {
            let mut chunk = sealed.chunk(offset, 8192);
            out.write_all(&chunk)?;
            offset += (chunk.len() / 2) as u32;
            chunk.fill(0);
        }
        out.flush()?;
        sealed.erase();
        self.pcm_bytes = data_len as usize;
        self.retained_at = Some(Instant::now());
        Ok(true)
    }

    pub fn read_pcm_if_valid(&mut self) -> Result<Option<Vec<u8>>> {
        if self.expired() {
            self.delete()?;
            return Ok(None);
        }
        if self.pcm_bytes == 0 || !self.file_path.exists() {
            return Ok(None);
        }
        let raw = fs::read(&self.file_path)?;
        if raw.len() <= 44 {
            return Ok(None);
        }
        Ok(Some(raw[44..].to_vec()))
    }

    pub fn available(&mut self) -> bool {
        if self.expired() {
            let _ = self.delete();
            return false;
        }
        self.pcm_bytes > 0 && self.file_path.is_file()
    }

    pub fn expired(&self) -> bool {
        match self.retained_at {
            Some(ts) => ts.elapsed() >= DIAGNOSTIC_TTL,
            None => false,
        }
    }

    pub fn delete(&mut self) -> Result<()> {
        self.pcm_bytes = 0;
        self.retained_at = None;
        if self.file_path.exists() {
            fs::remove_file(&self.file_path)?;
        }
        Ok(())
    }
}

impl Drop for PrivateDiagnosticClip {
    fn drop(&mut self) {
        let _ = self.delete();
    }
}

pub fn wav_header(data_bytes: u32, sample_rate: u32, channels: u16) -> [u8; 44] {
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(data_bytes + 36).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_clip_rejects_non_diagnostic_purpose() {
        let dir = std::env::temp_dir().join("nexo_diag_test_reject");
        let mut clip = PrivateDiagnosticClip::new(&dir).unwrap();
        let mut conv = Sealed::new(vec![1u8; 100], "conversation", "manual_finish", 0);
        assert!(!clip.retain(&mut conv, true).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnostic_clip_retains_erases_source_and_deletes_on_drop() {
        let dir = std::env::temp_dir().join("nexo_diag_test_ok");
        let wav_path = dir.join("local-test.wav");
        {
            let mut clip = PrivateDiagnosticClip::new(&dir).unwrap();
            let mut diag = Sealed::new(vec![0x10u8; 640], "diagnostic", "manual_finish", 0);
            assert!(clip.retain(&mut diag, true).unwrap());
            assert!(diag.is_released());
            assert!(wav_path.is_file());
        }
        assert!(!wav_path.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
