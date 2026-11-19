//! Half-duplex microphone gate.
//!
//! Ported from `android/.../audio/HalfDuplexGate.kt`. The rule is simple:
//! microphone frames are discarded while the server works or while playback
//! drains. Only an explicit handshake (`connected()`) may re-open capture, and
//! only after an explicit `microphoneReady(true)` does `can_send_audio` become
//! true. Every mutator is serialized through a single mutex, mirroring the
//! Kotlin `@Synchronized` methods so the capture thread always observes a
//! complete transition rather than a half-written snapshot.

use std::sync::Mutex;

/// Lifecycle state of the half-duplex gate. Mirrors `NexoState` at the subset
/// the gate actually inspects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateState {
    Disconnected,
    Idle,
    Listening,
    Thinking,
    Speaking,
    Error,
}

/// Thread-safe half-duplex gate. All state transitions are atomic w.r.t. the
/// capture thread.
#[derive(Debug)]
pub struct HalfDuplexGate {
    inner: Mutex<Gate>,
}

#[derive(Debug)]
struct Gate {
    ready: bool,
    microphone: bool,
    state: GateState,
    playback_pending: bool,
}

impl Default for HalfDuplexGate {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Gate {
                ready: false,
                microphone: false,
                state: GateState::Disconnected,
                playback_pending: false,
            }),
        }
    }
}

impl HalfDuplexGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true only when the gate is in a state where the capture thread
    /// may forward PCM to the upload pipeline. This is the single source of
    /// truth the audio loop consults before accepting a frame.
    pub fn can_send_audio(&self) -> bool {
        let g = self.inner.lock().expect("half-duplex mutex poisoned");
        g.ready && g.microphone && !g.playback_pending && matches!(g.state, GateState::Idle | GateState::Listening)
    }

    /// Duplicate acknowledgement must not reopen a busy turn. Only a fresh
    /// handshake (from `disconnected`) transitions the gate into a usable state.
    pub fn connected(&self) {
        let mut g = self.inner.lock().expect("half-duplex mutex poisoned");
        if g.ready {
            return;
        }
        g.microphone = false;
        g.state = GateState::Idle;
        g.playback_pending = false;
        g.ready = true;
    }

    /// Full reset. After this call the gate refuses audio until a new
    /// `connected()` is issued.
    pub fn disconnected(&self) {
        let mut g = self.inner.lock().expect("half-duplex mutex poisoned");
        g.ready = false;
        g.microphone = false;
        g.playback_pending = false;
        g.state = GateState::Disconnected;
    }

    pub fn microphone_ready(&self, ready: bool) {
        let mut g = self.inner.lock().expect("half-duplex mutex poisoned");
        g.microphone = ready;
    }

    pub fn state_changed(&self, state: GateState) {
        let mut g = self.inner.lock().expect("half-duplex mutex poisoned");
        g.state = state;
    }

    pub fn tts_started(&self) {
        let mut g = self.inner.lock().expect("half-duplex mutex poisoned");
        g.playback_pending = true;
    }

    pub fn playback_drained(&self) {
        let mut g = self.inner.lock().expect("half-duplex mutex poisoned");
        g.playback_pending = false;
    }

    /// A failed output ends the turn, but only a new handshake may re-enable
    /// capture — mirrors `playbackFailed()` calling `disconnected()`.
    pub fn playback_failed(&self) {
        self.disconnected();
    }

    /// Read-only snapshot for diagnostics / logging.
    pub fn snapshot(&self) -> GateSnapshot {
        let g = self.inner.lock().expect("half-duplex mutex poisoned");
        GateSnapshot {
            ready: g.ready,
            microphone: g.microphone,
            state: g.state,
            playback_pending: g.playback_pending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateSnapshot {
    pub ready: bool,
    pub microphone: bool,
    pub state: GateState,
    pub playback_pending: bool,
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_gate_blocks_audio() {
        let gate = HalfDuplexGate::new();
        assert!(!gate.can_send_audio());
    }

    #[test]
    fn connected_without_microphone_still_blocks() {
        let gate = HalfDuplexGate::new();
        gate.connected();
        // ready=true, state=Idle, but microphone is still false
        assert!(!gate.can_send_audio());
    }

    #[test]
    fn connected_plus_mic_ready_opens_gate() {
        let gate = HalfDuplexGate::new();
        gate.connected();
        gate.microphone_ready(true);
        assert!(gate.can_send_audio());
    }

    #[test]
    fn tts_start_closes_gate_until_drained() {
        let gate = HalfDuplexGate::new();
        gate.connected();
        gate.microphone_ready(true);
        assert!(gate.can_send_audio());

        gate.tts_started();
        assert!(!gate.can_send_audio());

        gate.playback_drained();
        assert!(gate.can_send_audio());
    }

    #[test]
    fn thinking_state_blocks_audio() {
        let gate = HalfDuplexGate::new();
        gate.connected();
        gate.microphone_ready(true);
        gate.state_changed(GateState::Thinking);
        assert!(!gate.can_send_audio());
    }

    #[test]
    fn speaking_state_blocks_audio() {
        let gate = HalfDuplexGate::new();
        gate.connected();
        gate.microphone_ready(true);
        gate.state_changed(GateState::Speaking);
        assert!(!gate.can_send_audio());
    }

    #[test]
    fn disconnected_resets_everything() {
        let gate = HalfDuplexGate::new();
        gate.connected();
        gate.microphone_ready(true);
        assert!(gate.can_send_audio());

        gate.disconnected();
        assert!(!gate.can_send_audio());

        // A duplicate connected() after disconnect is allowed (fresh handshake),
        // but the gate still requires microphone_ready before opening.
        gate.connected();
        assert!(!gate.can_send_audio());
        gate.microphone_ready(true);
        assert!(gate.can_send_audio());
    }

    #[test]
    fn duplicate_connected_does_not_reopen_busy_turn() {
        let gate = HalfDuplexGate::new();
        gate.connected();
        gate.microphone_ready(true);
        gate.state_changed(GateState::Speaking);

        // Duplicate connected() is a no-op while ready; must not flip state.
        gate.connected();
        assert!(!gate.can_send_audio());
    }

    #[test]
    fn playback_failed_disconnects() {
        let gate = HalfDuplexGate::new();
        gate.connected();
        gate.microphone_ready(true);
        gate.playback_failed();
        assert!(!gate.can_send_audio());
    }
}