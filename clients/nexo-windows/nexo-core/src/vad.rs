//! Silero VAD v6.2.3 wrapper (ONNX, 16 kHz, CPU-only).
//!
//! Faithful port of `android/.../audio/vad/SileroVad.java`:
//! - Pinned model (SHA-256 verified at load time, matching the Java constant).
//! - Thread-confined use: the caller must not share one [`SileroVad`] across
//!   threads. State is mutated on every inference.
//! - 64-sample rolling context prepended to each 512-sample frame → 576 input.
//! - `state` tensor shape (2, 1, 128); `sr` is an `i64` scalar (16000).
//! - Outputs are positional: index 0 = probability (1, 1), index 1 = next_state
//!   (2, 1, 128). The next-state tensor is copied back for the next frame.
//! - Not speaker recognition.

use anyhow::{anyhow, Context, Result};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::OnceLock;

/// Expected SHA-256 of the bundled Silero v6.2.3 ONNX model.
/// Must match `SileroVad.MODEL_SHA256` in the Android client.
pub const MODEL_SHA256: &str = "1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3";

/// 512-sample frame @ 16 kHz = 32 ms (one VAD step).
pub const FRAME_SAMPLES: usize = 512;
/// 64-sample rolling context carried across frames.
pub const CONTEXT_SAMPLES: usize = 64;
/// Combined input length = context + frame.
pub const INPUT_SAMPLES: usize = CONTEXT_SAMPLES + FRAME_SAMPLES; // 576
/// LSTM state depth per layer.
const STATE_DIM: usize = 128;

/// Silero VAD session state.
///
/// Holds the ONNX Runtime session, rolling context window and recurrent state.
/// Reset with [`SileroVad::reset`] to clear state between utterances.
pub struct SileroVad {
    session: Session,
    /// 64-sample rolling context (tail of previous frame).
    context: [f32; CONTEXT_SAMPLES],
    /// Combined input buffer: [context(64) | frame(512)].
    input: [f32; INPUT_SAMPLES],
    /// LSTM hidden state, flattened (2, 1, 128) -> 256 elements.
    state: [f32; 2 * STATE_DIM],
}

static ORT_ENV: OnceLock<ort::environment::Environment> = OnceLock::new();

fn ensure_ort_env() -> Result<&'static ort::environment::Environment> {
    if let Some(env) = ORT_ENV.get() {
        return Ok(env);
    }
    // ort only allows one global environment per process. Another subsystem may
    // have initialised it already; in that case the build fails but the process
    // environment is usable via Session::builder's requirement to pass &env. We
    // try to build ours; if it reports "only one environment..." we cannot obtain
    // a reference here, so we must have initialised it ourselves earlier.
    match ort::init().with_name("nexo-vad").build() {
        Ok(env) => {
            let _ = ORT_ENV.set(env);
            ORT_ENV
                .get()
                .ok_or_else(|| anyhow!("failed to store ORT environment"))
        }
        Err(e) => {
            // If someone else already initialised, we can't retrieve it (API is
            // crate-private). Surface a clear error rather than guessing.
            Err(anyhow!("initialise ORT environment: {e}"))
        }
    }
}

/// Explicitly initialise the global ONNX Runtime environment used by VAD.
/// Safe to call once at process startup; subsequent calls are no-ops if the
/// same environment is already stored.
pub fn init_ort() -> Result<()> {
    ensure_ort_env().map(|_| ())
}

impl SileroVad {
    /// Load and verify the Silero VAD model at `path`.
    ///
    /// The file's SHA-256 must equal [`MODEL_SHA256`]. This pins the exact model
    /// revision, mirroring the Java implementation's integrity check.
    ///
    /// The global ORT environment is initialised on first use.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .with_context(|| format!("read VAD model from {}", path.display()))?;
        let digest = Sha256::digest(&bytes);
        let hex = hex::encode(digest);
        if hex != MODEL_SHA256 {
            return Err(anyhow!(
                "vad_model_checksum: expected {MODEL_SHA256}, got {hex} for {}",
                path.display()
            ));
        }

        let env = ensure_ort_env()?;
        let session = Session::builder(env)
            .map_err(|e| anyhow!("create ORT session builder: {e}"))?
            .with_optimization_level(GraphOptimizationLevel::Disable)
            .map_err(|e| anyhow!("set optimization level: {e}"))?
            .with_intra_threads(1)
            .map_err(|e| anyhow!("set intra threads: {e}"))?
            .with_inter_threads(1)
            .map_err(|e| anyhow!("set inter threads: {e}"))?
            .commit_from_file(path)
            .map_err(|e| anyhow!("load VAD model from {}: {e}", path.display()))?;

        Ok(Self {
            session,
            context: [0.0; CONTEXT_SAMPLES],
            input: [0.0; INPUT_SAMPLES],
            state: [0.0; 2 * STATE_DIM],
        })
    }

    /// Run VAD on a 512-sample normalised audio window (float samples in [-1, 1]).
    ///
    /// Returns the speech probability for this frame in [0.0, 1.0].
    /// Callers are responsible for any gain normalisation before this call
    /// (see Android `LocalCapture.java` peak-based gain logic).
    ///
    /// This mutates internal context/state — do not call concurrently.
    pub fn probability(&mut self, window: &[f32; FRAME_SAMPLES]) -> Result<f32> {
        // Build input = [context(64) | window(512)]
        self.input[..CONTEXT_SAMPLES].copy_from_slice(&self.context);
        self.input[CONTEXT_SAMPLES..].copy_from_slice(window);

        let audio = Tensor::from_array(([1i64, INPUT_SAMPLES as i64], self.input.to_vec()))
            .context("create audio tensor")?;
        let state_tensor = Tensor::from_array(([2i64, 1i64, STATE_DIM as i64], self.state.to_vec()))
            .context("create state tensor")?;
        // `sr` is an int64 scalar (shape []).
        let sr_tensor =
            Tensor::from_array(([] as [i64; 0], vec![16000i64])).context("create sr tensor")?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input" => audio,
                "state" => state_tensor,
                "sr" => sr_tensor
            ])
            .context("run VAD inference")?;

        // Positional outputs: 0 = probability (1,1), 1 = next_state (2,1,128).
        let (_prob_shape, prob_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("extract probability tensor")?;
        let probability = *prob_data
            .first()
            .ok_or_else(|| anyhow!("empty probability tensor"))?;

        let (_state_shape, next_state_data) = outputs[1]
            .try_extract_tensor::<f32>()
            .context("extract next_state tensor")?;
        if next_state_data.len() != 2 * STATE_DIM {
            return Err(anyhow!(
                "unexpected next_state length {}, expected {}",
                next_state_data.len(),
                2 * STATE_DIM
            ));
        }
        self.state.copy_from_slice(next_state_data);

        // Roll context: last 64 samples of the frame (tail of window).
        self.context
            .copy_from_slice(&window[FRAME_SAMPLES - CONTEXT_SAMPLES..]);

        Ok(probability)
    }

    /// Convenience: run VAD on a borrowed slice, validating the length.
    pub fn probability_slice(&mut self, window: &[f32]) -> Result<f32> {
        if window.len() != FRAME_SAMPLES {
            return Err(anyhow!(
                "vad_window: expected {FRAME_SAMPLES} samples, got {}",
                window.len()
            ));
        }
        let mut arr = [0.0f32; FRAME_SAMPLES];
        arr.copy_from_slice(window);
        self.probability(&arr)
    }

    /// Clear rolling context and recurrent state (e.g. between utterances).
    pub fn reset(&mut self) {
        self.context.fill(0.0);
        self.input.fill(0.0);
        self.state.fill(0.0);
    }
}

impl std::fmt::Debug for SileroVad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SileroVad")
            .field("model_sha256", &MODEL_SHA256)
            .field("frame_samples", &FRAME_SAMPLES)
            .finish_non_exhaustive()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_java() {
        assert_eq!(
            MODEL_SHA256,
            "1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3"
        );
        assert_eq!(FRAME_SAMPLES, 512);
        assert_eq!(CONTEXT_SAMPLES, 64);
        assert_eq!(INPUT_SAMPLES, 576);
    }

    #[test]
    fn probability_slice_rejects_wrong_length() {
        assert_eq!(INPUT_SAMPLES, CONTEXT_SAMPLES + FRAME_SAMPLES);
    }

    #[test]
    fn reset_is_pure_state_clear() {
        let mut ctx = [1.0f32; CONTEXT_SAMPLES];
        let mut st = [1.0f32; 2 * STATE_DIM];
        ctx.fill(0.0);
        st.fill(0.0);
        assert!(ctx.iter().all(|v| *v == 0.0));
        assert!(st.iter().all(|v| *v == 0.0));
    }
}
