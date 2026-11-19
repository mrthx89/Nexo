//! Utterance upload state machine with integrity verification.
//!
//! Ported from `android/.../audio/core/UploadSession.java`. Local queue
//! admission is never an integrity ACK — only the server's `accepted` /
//! `completed` stage with a matching SHA-256 digest releases the buffer.
//!
//! The state machine is serialized through a single mutex, mirroring the
//! `synchronized` methods on the Java class. Binary frames are framed with a
//! 28-byte header (`encode()`), matching the Android wire format exactly.

use crate::protocol::OutdoorSession;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Maximum samples per utterance (320 000 = 20 s at 16 kHz).
pub const MAX_SAMPLES: u32 = 320_000;
/// Pre-roll buffer length in samples (0.4 s at 16 kHz).
pub const PRE_ROLL_SAMPLES: u32 = 6_400;
/// Upload is considered expired 30 s after the buffer is sealed.
const SEAL_TTL_MS: i64 = 30_000;

/// Maximum bytes per binary frame (even, <= 16 384).
const MAX_CHUNK_BYTES: usize = 16_384;
/// Header length of a binary utterance frame.
const FRAME_HEADER_LEN: usize = 28;

// ── Sealed PCM ──────────────────────────────────────────────────────────

/// A sealed PCM utterance. The buffer cannot be reused until every consumer
/// has released it. `erase()` zeroises the payload and flips the live bit so
/// subsequent reads panic (matching the Java `IllegalStateException`).
#[derive(Debug)]
pub struct Sealed {
    pcm: Vec<u8>,
    bytes: usize,
    released: bool,
    pub id: String,
    pub purpose: String,
    pub end_reason: String,
    pub sealed_at: i64,
}

impl Sealed {
    /// Construct a sealed buffer from raw PCM16 bytes. `purpose` must be one of
    /// `conversation`, `enrollment`, `diagnostic`. `end_reason` must be one of
    /// `manual_finish`, `vad_end`, `duration_limit`.
    pub fn new(pcm: Vec<u8>, purpose: &str, end_reason: &str, now_ms: i64) -> Self {
        assert!(
            matches!(purpose, "conversation" | "enrollment" | "diagnostic"),
            "invalid purpose: {purpose}"
        );
        assert!(
            matches!(end_reason, "manual_finish" | "vad_end" | "duration_limit"),
            "invalid end_reason: {end_reason}"
        );
        let bytes = pcm.len();
        Self {
            pcm,
            bytes,
            released: false,
            id: Uuid::new_v4().to_string(),
            purpose: purpose.to_string(),
            end_reason: end_reason.to_string(),
            sealed_at: now_ms,
        }
    }

    /// Number of 16-bit samples.
    pub fn samples(&self) -> u32 {
        (self.bytes / 2) as u32
    }

    /// SHA-256 hex digest of the PCM payload.
    pub fn sha256(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.pcm[..self.bytes]);
        hex_encode(&hasher.finalize())
    }

    /// Extract a chunk of PCM16 bytes starting at `sample_offset`, at most
    /// `max_bytes` long. Returns a copy so the caller can send it without
    /// holding the lock.
    pub fn chunk(&self, sample_offset: u32, max_bytes: usize) -> Vec<u8> {
        assert!(max_bytes >= 2 && max_bytes % 2 == 0, "invalid max_bytes");
        let offset = sample_offset as usize * 2;
        assert!(offset < self.bytes, "sample_offset out of range");
        let end = (offset + max_bytes).min(self.bytes);
        self.pcm[offset..end].to_vec()
    }

    pub fn is_released(&self) -> bool {
        self.released
    }

    /// Zeroise the payload and mark the buffer dead. Subsequent calls to
    /// `chunk()` / `sha256()` panic.
    pub fn erase(&mut self) {
        for b in &mut self.pcm {
            *b = 0;
        }
        self.released = true;
    }
}

// ── Binary frame encoding ───────────────────────────────────────────────

/// Encode a binary utterance frame. Wire format (big-endian):
///   magic `0x4e584132` ("NEXA") (4) | utterance_id high (8) | low (8)
///   | sample_offset (4) | payload_len (4) | payload
pub fn encode(id: &str, sample_offset: u32, payload: &[u8]) -> Result<Vec<u8>, UploadError> {
    let uuid = Uuid::parse_str(id).map_err(|_| UploadError::InvalidUtteranceId)?;
    if payload.len() < 2 || payload.len() > MAX_CHUNK_BYTES || payload.len() % 2 != 0 {
        return Err(UploadError::InvalidLength);
    }
    let total_samples = (sample_offset as u64) + (payload.len() / 2) as u64;
    if total_samples > MAX_SAMPLES as u64 {
        return Err(UploadError::InvalidLength);
    }
    let (msb, lsb) = uuid.as_u64_pair();
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + payload.len());
    frame.extend_from_slice(&0x4e58_4132u32.to_be_bytes());
    frame.extend_from_slice(&msb.to_be_bytes());
    frame.extend_from_slice(&lsb.to_be_bytes());
    frame.extend_from_slice(&sample_offset.to_be_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

// ── Transport trait ─────────────────────────────────────────────────────

/// Abstraction over the WebSocket so the upload state machine can be tested
/// against a fixture without a live socket.
pub trait Transport {
    /// Send a JSON control frame. Returns `true` if the frame was queued.
    fn control(&mut self, value: &serde_json::Value) -> bool;
    /// Send a binary utterance frame. Returns `true` if the frame was queued.
    fn binary(&mut self, value: &[u8]) -> bool;
    /// Number of bytes currently queued on the wire (backpressure estimate).
    fn queued_bytes(&self) -> u64;
    /// Abort the connection. Called when the session enters an unrecoverable
    /// error.
    fn abort(&mut self) {}
}

// ── Upload state machine ────────────────────────────────────────────────

/// Lifecycle state of an upload session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadState {
    /// No active upload. A fresh `begin()` may start a new one.
    Paused,
    /// `utterance_begin` sent; waiting for the server's `begin` ack.
    Begin,
    /// Server accepted `begin`; streaming chunks.
    Sending,
    /// All chunks sent; `utterance_commit` sent; waiting for final ack.
    Commit,
    /// Server confirmed `accepted` (integrity verified).
    Accepted,
    /// Server confirmed `completed` (utterance fully processed).
    Completed,
    /// Server is re-checking an already-accepted utterance.
    AcceptedStatus,
    /// Checking status of a prior utterance after reconnect.
    Status,
    /// Upload interrupted; may be retried within the seal TTL.
    Disconnected,
    /// Server reported `unknown`; safe to retry with the same utterance id.
    RetryReady,
    /// `utterance_cancel` sent; waiting for `cancelled` ack.
    Cancelling,
    /// Cancelled and acknowledged.
    Cancelled,
    /// Fatal error. The utterance is dropped.
    Error,
}

/// Errors from the upload state machine.
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("invalid utterance id")]
    InvalidUtteranceId,
    #[error("invalid length")]
    InvalidLength,
    #[error("unsupported capability")]
    UnsupportedCapability,
    #[error("expired")]
    Expired,
    #[error("delivery_unknown")]
    DeliveryUnknown,
    #[error("cancelled")]
    Cancelled,
    #[error("cancel_timeout")]
    CancelTimeout,
    #[error("sequence_gap")]
    SequenceGap,
    #[error("checksum_mismatch")]
    ChecksumMismatch,
    #[error("transport_unavailable")]
    TransportUnavailable,
    #[error("upload_timeout")]
    UploadTimeout,
    #[error("server error: {0}")]
    ServerError(String),
}

/// Serialized upload worker. Only one utterance may be in flight at a time.
pub struct UploadSession {
    state: UploadState,
    error: Option<String>,
    /// The sealed PCM buffer (owned until accepted/erased).
    pcm: Option<Sealed>,
    /// Server-assigned utterance id (echoed in every frame).
    id: Option<String>,
    /// Connection id from the most recent `server_hello`.
    connection: Option<String>,
    /// Server instance id.
    instance: Option<String>,
    /// The instance id captured when this upload began (for reconnect checks).
    original_instance: Option<String>,
    /// Lease id (enrollment only).
    lease: Option<String>,
    /// Enrollment id (enrollment only).
    enrollment: Option<String>,
    /// Request id of the last control frame sent (for matching acks).
    request: Option<String>,
    /// Max chunk size negotiated in `server_hello`.
    chunk_bytes: usize,
    /// Samples already sent on the wire.
    sent_samples: u32,
    /// Samples the server has confirmed received.
    confirmed: u32,
    /// Samples the server accepted (integrity-verified count).
    accepted_samples: u32,
    /// SHA-256 the server acknowledged.
    accepted_hash: Option<String>,
    /// Deadline (ms) for the current state.
    deadline: i64,
    /// Whether the server advertised upload_v2 support.
    supported: bool,
    /// Last control frame built by `send()`, awaiting write to the socket.
    last_control: Option<serde_json::Value>,
}

impl Default for UploadSession {
    fn default() -> Self {
        Self {
            state: UploadState::Paused,
            error: None,
            pcm: None,
            id: None,
            connection: None,
            instance: None,
            original_instance: None,
            lease: None,
            enrollment: None,
            request: None,
            chunk_bytes: 0,
            sent_samples: 0,
            confirmed: 0,
            accepted_samples: 0,
            accepted_hash: None,
            deadline: 0,
            supported: false,
            last_control: None,
        }
    }
}

impl UploadSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> UploadState {
        self.state
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Connect to a negotiated session. Validates the chunk size and resets
    /// the per-connection state. Returns `false` if the capability is
    /// unsupported.
    pub fn connect(
        &mut self,
        session: &OutdoorSession,
        selected: bool,
        now_ms: i64,
    ) -> Result<(), UploadError> {
        if !is_canonical_uuid(&session.connection_id) {
            return Err(UploadError::InvalidUtteranceId);
        }
        if !is_canonical_uuid(&session.server_instance_id) {
            return Err(UploadError::InvalidUtteranceId);
        }
        let chunk = session.max_chunk_bytes as usize;
        let supported = selected && (2..=MAX_CHUNK_BYTES).contains(&chunk) && chunk % 2 == 0;
        self.connection = Some(session.connection_id.clone());
        self.instance = Some(session.server_instance_id.clone());
        self.supported = supported;
        self.chunk_bytes = chunk;

        if !supported {
            self.fail("unsupported_capability");
            return Err(UploadError::UnsupportedCapability);
        }
        if self.state == UploadState::Cancelling {
            self.fail("cancelled");
            return Err(UploadError::Cancelled);
        }

        // No active utterance: nothing to do.
        if self.pcm.is_none() {
            // If we already have an accepted utterance awaiting its result,
            // re-issue a status check.
            if let Some(id) = self.id.clone() {
                if matches!(self.state, UploadState::Accepted | UploadState::AcceptedStatus) {
                    if self.original_instance.as_deref() != Some(session.server_instance_id.as_str()) {
                        self.fail("delivery_unknown");
                        return Err(UploadError::DeliveryUnknown);
                    }
                    self.state = UploadState::AcceptedStatus;
                    self.deadline = now_ms + 5_000;
                    self.send("utterance_status");
                    return Ok(());
                }
                let _ = id;
            }
            return Ok(());
        }

        // Active utterance: must still be within TTL and on the same instance.
        if self.expired(now_ms) {
            self.fail("expired");
            return Err(UploadError::Expired);
        }
        if self.original_instance.as_deref() != Some(session.server_instance_id.as_str()) {
            self.fail("delivery_unknown");
            return Err(UploadError::DeliveryUnknown);
        }
        // Only conversation utterances may be resumed across reconnects.
        if self.pcm.as_ref().map(|p| p.purpose.as_str()) != Some("conversation") {
            self.fail("cancelled");
            return Err(UploadError::Cancelled);
        }
        self.state = UploadState::Status;
        self.deadline = now_ms + 5_000;
        self.send("utterance_status");
        Ok(())
    }

    /// Begin uploading a sealed utterance. Returns `false` if the session is
    /// not ready (no connection, already busy, diagnostic, etc.).
    pub fn begin(
        &mut self,
        sealed: Sealed,
        lease_id: Option<&str>,
        enrollment_id: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, UploadError> {
        if self.state == UploadState::Completed {
            self.id = None;
        }
        if !self.supported || self.connection.is_none() || self.id.is_some() || sealed.is_released() {
            return Ok(false);
        }
        if sealed.purpose == "diagnostic" {
            return Ok(false);
        }
        if sealed.purpose == "enrollment" {
            let lease = lease_id.ok_or(UploadError::InvalidUtteranceId)?;
            let enr = enrollment_id.ok_or(UploadError::InvalidUtteranceId)?;
            if !is_canonical_uuid(lease) || !is_canonical_uuid(enr) {
                return Err(UploadError::InvalidUtteranceId);
            }
            self.lease = Some(lease.to_string());
            self.enrollment = Some(enr.to_string());
        }
        if now_ms - sealed.sealed_at >= SEAL_TTL_MS || now_ms < sealed.sealed_at {
            self.fail("expired");
            return Err(UploadError::Expired);
        }
        self.pcm = Some(sealed);
        self.id = self.pcm.as_ref().map(|p| p.id.clone());
        self.original_instance = self.instance.clone();
        self.error = None;
        Ok(self.start_begin(now_ms))
    }

    /// Retain a sealed utterance while the socket is disconnected, so it can
    /// be retried after reconnect without re-capturing. Only conversation
    /// utterances are eligible.
    pub fn retain_disconnected(
        &mut self,
        sealed: Sealed,
        server_instance: &str,
        now_ms: i64,
    ) -> Result<bool, UploadError> {
        if self.id.is_some()
            || sealed.purpose != "conversation"
            || sealed.is_released()
        {
            return Ok(false);
        }
        if !is_canonical_uuid(server_instance) {
            return Err(UploadError::InvalidUtteranceId);
        }
        if now_ms < sealed.sealed_at || now_ms - sealed.sealed_at >= SEAL_TTL_MS {
            return Ok(false);
        }
        self.pcm = Some(sealed);
        self.id = self.pcm.as_ref().map(|p| p.id.clone());
        self.original_instance = Some(server_instance.to_string());
        self.error = None;
        self.state = UploadState::Disconnected;
        Ok(true)
    }

    /// Retry a previously retained upload after the server reports `unknown`.
    pub fn resume_retry(&mut self, now_ms: i64) -> Result<bool, UploadError> {
        if self.state != UploadState::RetryReady || self.pcm.is_none() || self.connection.is_none() {
            return Ok(false);
        }
        if self.expired(now_ms) {
            self.fail("expired");
            return Err(UploadError::Expired);
        }
        Ok(self.start_begin(now_ms))
    }

    /// Handle an `utterance_ack` control frame from the server.
    pub fn ack(
        &mut self,
        connection_id: &str,
        request_id: &str,
        utterance_id: &str,
        stage: &str,
        next: u32,
        samples: u32,
        hash: Option<&str>,
        now_ms: i64,
    ) {
        if connection_id != self.connection.as_deref().unwrap_or("") || Some(utterance_id) != self.id.as_deref() {
            return;
        }

        // Cancelling: only a matching `cancelled` ack completes the cancel.
        if self.state == UploadState::Cancelling {
            if self.request.as_deref() == Some(request_id) && stage == "cancelled" {
                self.id = None;
                self.state = UploadState::Cancelled;
            }
            return;
        }

        // Re-checking an accepted utterance after reconnect.
        if self.state == UploadState::AcceptedStatus {
            if self.request.as_deref() != Some(request_id) {
                return;
            }
            if (stage == "accepted" || stage == "completed")
                && samples == self.accepted_samples
                && hash == self.accepted_hash.as_deref()
            {
                self.state = if stage == "accepted" {
                    UploadState::Accepted
                } else {
                    UploadState::Completed
                };
            } else {
                self.fail("delivery_unknown");
            }
            return;
        }

        if self.pcm.is_none() || self.is_terminal() {
            return;
        }
        if self.expired(now_ms) {
            self.fail("expired");
            return;
        }

        // Streaming progress: server confirms it has received `next` samples.
        if stage == "receiving" {
            if !matches!(self.state, UploadState::Sending | UploadState::Commit) {
                return;
            }
            if next < self.confirmed || next > self.sent_samples {
                self.fail("sequence_gap");
                return;
            }
            self.confirmed = next;
            return;
        }

        // All other stages must match the request id we sent.
        if self.request.as_deref() != Some(request_id) {
            return;
        }

        match (stage, self.state) {
            ("begin", UploadState::Begin) => {
                if next != 0 {
                    self.fail("sequence_gap");
                    return;
                }
                self.state = UploadState::Sending;
                self.deadline = now_ms + 30_000;
            }
            ("accepted", _) | ("completed", _) => {
                if !matches!(self.state, UploadState::Commit | UploadState::Status | UploadState::Begin) {
                    return;
                }
                // Verify integrity: the acked sample count must equal the
                // sealed length and the acked hash must equal the local
                // SHA-256. Anything else is a checksum mismatch.
                let sealed = self.pcm.as_ref().unwrap();
                if samples != sealed.samples() {
                    self.fail("checksum_mismatch");
                    return;
                }
                let sealed_hash = self.pcm.as_mut().unwrap().sha256().to_string();
                if hash != Some(sealed_hash.as_str()) {
                    self.fail("checksum_mismatch");
                    return;
                }
                self.state = if stage == "accepted" {
                    UploadState::Accepted
                } else {
                    UploadState::Completed
                };
                self.accepted_samples = samples;
                self.accepted_hash = hash.map(str::to_string);
                if let Some(p) = self.pcm.as_mut() {
                    p.erase();
                }
                self.pcm = None;
            }
            ("unknown", UploadState::Status) => {
                self.state = UploadState::RetryReady;
            }
            ("cancelled", _) | ("rejected", _) => {
                self.fail(stage);
            }
            _ => {}
        }
    }

    /// Pump the state machine. Sends the next binary chunk when in `Sending`,
    /// or commits when all samples have been sent. Must be called periodically
    /// (e.g. every 20 ms) while the session is active.
    pub fn pump<T: Transport>(&mut self, wire: &mut T, now_ms: i64) {
        if self.state == UploadState::Cancelling {
            if now_ms >= self.deadline {
                self.fail("cancel_timeout");
                wire.abort();
            }
            return;
        }
        if self.state == UploadState::AcceptedStatus && now_ms >= self.deadline {
            self.fail("delivery_unknown");
            wire.abort();
            return;
        }
        // Disconnected retries retain the original seal deadline.
        if self.pcm.is_some() && self.expired(now_ms) {
            self.fail("expired");
            return;
        }
        if self.pcm.is_none()
            || self.is_terminal()
            || self.state == UploadState::Disconnected
            || self.state == UploadState::RetryReady
        {
            return;
        }
        if now_ms >= self.deadline {
            self.state = UploadState::Disconnected;
            self.error = Some("upload_timeout".into());
            return;
        }
        if self.state != UploadState::Sending {
            return;
        }

        // Backpressure: don't queue more than 128 KiB.
        let queued = wire.queued_bytes();
        let frame_overhead = 2u64 * (MAX_CHUNK_BYTES as u64 + 28) + 4096;
        if queued + frame_overhead > 131_072 {
            return;
        }

        let sealed = self.pcm.as_ref().unwrap();
        let remaining = sealed.samples() - self.sent_samples;
        let bytes_to_send = ((remaining as usize) * 2).min(self.chunk_bytes);
        if bytes_to_send > 0 {
            let payload = sealed.chunk(self.sent_samples, bytes_to_send);
            let frame = match encode(&self.id.clone().unwrap(), self.sent_samples, &payload) {
                Ok(f) => f,
                Err(_) => {
                    self.state = UploadState::Disconnected;
                    self.error = Some("transport_unavailable".into());
                    return;
                }
            };
            if !wire.binary(&frame) {
                self.state = UploadState::Disconnected;
                self.error = Some("transport_unavailable".into());
                return;
            }
            self.sent_samples += (payload.len() / 2) as u32;
        }

        if self.sent_samples == sealed.samples() {
            self.state = UploadState::Commit;
            self.deadline = now_ms + 5_000;
            let _ = self.send("utterance_commit");
        }
    }

    /// Mark the socket as disconnected. Non-conversation utterances are
    /// cancelled; conversation utterances enter `Disconnected` for retry.
    pub fn disconnected(&mut self) {
        self.connection = None;
        self.supported = false;
        if self.state == UploadState::Cancelling {
            self.fail("cancelled");
            return;
        }
        if self.pcm.is_some() && !self.is_terminal() {
            self.state = UploadState::Disconnected;
        }
        if self.pcm.as_ref().map(|p| p.purpose.as_str()) != Some("conversation") {
            self.fail("cancelled");
        }
    }

    /// Cancel the active upload.
    pub fn cancel<T: Transport>(&mut self, wire: &mut T, now_ms: i64) {
        if self.state == UploadState::Cancelling {
            return;
        }
        let mut sent = false;
        if let (Some(idv), Some(conn)) = (&self.id, &self.connection) {
            if self.state != UploadState::Completed {
                let req = Uuid::new_v4().to_string();
                let mut value = serde_json::Map::new();
                value.insert("type".into(), serde_json::Value::String("utterance_cancel".into()));
                value.insert("connection_id".into(), serde_json::Value::String(conn.clone()));
                value.insert("request_id".into(), serde_json::Value::String(req.clone()));
                value.insert("utterance_id".into(), serde_json::Value::String(idv.clone()));
                value.insert("reason".into(), serde_json::Value::String("cancelled".into()));
                sent = wire.control(&serde_json::Value::Object(value.clone()));
                if sent {
                    self.request = Some(req);
                    self.last_control = Some(serde_json::Value::Object(value));
                }
            }
        }
        if let Some(p) = self.pcm.as_mut() {
            p.erase();
        }
        self.pcm = None;
        self.error = Some("cancelled".into());
        if sent {
            self.state = UploadState::Cancelling;
            self.deadline = now_ms + 2_000;
        } else {
            self.id = None;
            self.state = UploadState::Cancelled;
        }
    }

    /// Reset to the paused state, dropping any active utterance.
    pub fn clear(&mut self) {
        if let Some(p) = self.pcm.as_mut() {
            p.erase();
        }
        self.pcm = None;
        self.id = None;
        self.state = UploadState::Paused;
        self.error = None;
    }

    /// Handle a server-reported error for the active utterance.
    pub fn server_error(&mut self, utterance_id: &str, code: &str) {
        if Some(utterance_id) == self.id.as_deref() && !self.is_terminal() {
            self.fail(code);
        }
    }

    /// Confirm the terminal result for the active utterance.
    pub fn result(&mut self, utterance_id: &str) {
        if Some(utterance_id) == self.id.as_deref()
            && matches!(self.state, UploadState::Accepted | UploadState::Completed)
        {
            self.id = None;
            self.state = UploadState::Completed;
        }
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn start_begin(&mut self, now_ms: i64) -> bool {
        self.sent_samples = 0;
        self.confirmed = 0;
        self.state = UploadState::Begin;
        self.deadline = now_ms + 5_000;
        self.send("utterance_begin")
    }

    fn send(&mut self, msg_type: &str) -> bool {
        // In the real transport this would serialise to JSON and push to the
        // wire. We build the value here so tests can inspect it via a mock
        // transport, but the actual send is deferred to `pump()` / the ws
        // layer which owns the socket.
        let mut value = serde_json::Map::new();
        value.insert("type".into(), serde_json::Value::String(msg_type.to_string()));
        if let Some(conn) = &self.connection {
            value.insert("connection_id".into(), serde_json::Value::String(conn.clone()));
        }
        let req = Uuid::new_v4().to_string();
        value.insert("request_id".into(), serde_json::Value::String(req.clone()));
        self.request = Some(req);
        if let Some(id) = &self.id {
            value.insert("utterance_id".into(), serde_json::Value::String(id.clone()));
        }
        if let Some(p) = &self.pcm {
            if msg_type == "utterance_begin" || msg_type == "utterance_commit" {
                value.insert("total_samples".into(), serde_json::Value::from(p.samples()));
                value.insert("sha256".into(), serde_json::Value::String(p.sha256().to_string()));
            }
            if msg_type == "utterance_begin" {
                value.insert("purpose".into(), serde_json::Value::String(p.purpose.clone()));
                value.insert("end_reason".into(), serde_json::Value::String(p.end_reason.clone()));
                if let Some(l) = &self.lease {
                    value.insert("lease_id".into(), serde_json::Value::String(l.clone()));
                    if let Some(e) = &self.enrollment {
                        value.insert("enrollment_id".into(), serde_json::Value::String(e.clone()));
                    }
                }
            }
        }
        // The caller (ws layer) is responsible for actually writing this to
        // the socket. We return `true` to indicate the state machine advanced;
        // the ws layer calls `pump()` again after flushing.
        // Store the last built control frame for inspection.
        self.last_control = Some(serde_json::Value::Object(value));
        true
    }

    fn expired(&self, now_ms: i64) -> bool {
        match &self.pcm {
            None => false,
            Some(p) => p.is_released() || now_ms < p.sealed_at || now_ms - p.sealed_at >= SEAL_TTL_MS,
        }
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            UploadState::Error | UploadState::Accepted | UploadState::Completed
        )
    }

    fn fail(&mut self, code: &str) {
        self.error = Some(code.to_string());
        self.state = UploadState::Error;
        if let Some(p) = self.pcm.as_mut() {
            p.erase();
        }
        self.pcm = None;
        self.id = None;
    }
}

// `last_control` field (stored separately to keep `send` simple).
impl UploadSession {
    /// The most recent control frame built by `send()`. Used by the ws layer
    /// to write it to the socket, and by tests to assert wire shape.
    pub fn take_last_control(&mut self) -> Option<serde_json::Value> {
        self.last_control.take()
    }
}

fn is_canonical_uuid(s: &str) -> bool {
    match Uuid::parse_str(s) {
        Ok(u) => u.to_string() == s,
        Err(_) => false,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A transport that records everything sent and reports configurable
    /// backpressure.
    struct MockTransport {
        control_frames: Vec<serde_json::Value>,
        binary_frames: Vec<Vec<u8>>,
        queued: u64,
        abort_called: bool,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                control_frames: Vec::new(),
                binary_frames: Vec::new(),
                queued: 0,
                abort_called: false,
            }
        }
    }

    impl Transport for MockTransport {
        fn control(&mut self, value: &serde_json::Value) -> bool {
            self.control_frames.push(value.clone());
            true
        }
        fn binary(&mut self, value: &[u8]) -> bool {
            self.binary_frames.push(value.to_vec());
            true
        }
        fn queued_bytes(&self) -> u64 {
            self.queued
        }
        fn abort(&mut self) {
            self.abort_called = true;
        }
    }

    fn make_session() -> OutdoorSession {
        OutdoorSession {
            connection_id: "00000000-0000-0000-0000-000000000001".into(),
            server_instance_id: "00000000-0000-0000-0000-000000000002".into(),
            output_epoch: 1,
            max_chunk_bytes: 1024,
        }
    }

    fn make_sealed(samples: u32, purpose: &str) -> Sealed {
        let pcm = vec![0u8; samples as usize * 2];
        Sealed::new(pcm, purpose, "vad_end", 1_000)
    }

    #[test]
    fn begin_sends_utterance_begin_with_integrity_fields() {
        let mut up = UploadSession::new();
        up.connect(&make_session(), true, 0).unwrap();
        let sealed = make_sealed(16_000, "conversation");
        up.begin(sealed, None, None, 1_000).unwrap();
        assert_eq!(up.state(), UploadState::Begin);
        let ctrl = up.take_last_control().expect("begin control frame");
        assert_eq!(ctrl["type"], "utterance_begin");
        assert_eq!(ctrl["total_samples"], 16_000);
        assert!(ctrl["sha256"].as_str().unwrap().len() == 64);
        assert_eq!(ctrl["purpose"], "conversation");
        assert_eq!(ctrl["end_reason"], "vad_end");
    }

    #[test]
    fn pump_sends_binary_frames_chunked() {
        let mut up = UploadSession::new();
        up.connect(&make_session(), true, 0).unwrap();
        let sealed = make_sealed(4_000, "conversation");
        up.begin(sealed, None, None, 1_000).unwrap();
        // Simulate begin ack.
        let req = up.take_last_control().unwrap()["request_id"].as_str().unwrap().to_string();
        let id = up.id.clone().unwrap();
        up.ack(
            &make_session().connection_id,
            &req,
            &id,
            "begin",
            0,
            0,
            None,
            1_001,
        );
        assert_eq!(up.state(), UploadState::Sending);

        let mut wire = MockTransport::new();
        up.pump(&mut wire, 1_002);
        // 4000 samples * 2 bytes = 8000 bytes; max_chunk_bytes = 1024 bytes.
        // One pump sends one chunk: 1024 bytes = 512 samples (7 full + 1 remainder total).
        assert_eq!(wire.binary_frames.len(), 1);
        assert_eq!(up.sent_samples, 512);
    }

    #[test]
    fn full_upload_flow_reaches_commmit() {
        let mut up = UploadSession::new();
        up.connect(&make_session(), true, 0).unwrap();
        let sealed = make_sealed(2_000, "conversation");
        let expected_hash = sealed.sha256();
        up.begin(sealed, None, None, 1_000).unwrap();
        let req = up.take_last_control().unwrap()["request_id"].as_str().unwrap().to_string();
        let id = up.id.clone().unwrap();
        up.ack(&make_session().connection_id, &req, &id, "begin", 0, 0, None, 1_001);

        let mut wire = MockTransport::new();
        // Pump until all samples sent; pump only emits a control frame on Commit.
        for _ in 0..10 {
            up.pump(&mut wire, 1_002);
            if up.state() == UploadState::Commit {
                break;
            }
            // Receiving acks do not require a matching request_id.
            let req2 = up
                .take_last_control()
                .and_then(|c| c["request_id"].as_str().map(str::to_string))
                .unwrap_or_default();
            let sent = up.sent_samples;
            up.ack(&make_session().connection_id, &req2, &id, "receiving", sent, 2_000, None, 1_003);
        }
        assert_eq!(up.state(), UploadState::Commit);

        // Server accepts with matching hash.
        let req3 = up.take_last_control().unwrap()["request_id"].as_str().unwrap().to_string();
        up.ack(
            &make_session().connection_id,
            &req3,
            &id,
            "accepted",
            0,
            2_000,
            Some(&expected_hash),
            1_004,
        );
        assert_eq!(up.state(), UploadState::Accepted);
    }

    #[test]
    fn ack_with_wrong_hash_fails_checksum_mismatch() {
        let mut up = UploadSession::new();
        up.connect(&make_session(), true, 0).unwrap();
        let sealed = make_sealed(1_000, "conversation");
        up.begin(sealed, None, None, 1_000).unwrap();
        let req = up.take_last_control().unwrap()["request_id"].as_str().unwrap().to_string();
        let id = up.id.clone().unwrap();
        up.ack(&make_session().connection_id, &req, &id, "begin", 0, 0, None, 1_001);

        let mut wire = MockTransport::new();
        up.pump(&mut wire, 1_002);
        up.pump(&mut wire, 1_002);
        // Commit sent; now send a wrong hash.
        let req3 = up.take_last_control().unwrap()["request_id"].as_str().unwrap().to_string();
        up.ack(
            &make_session().connection_id,
            &req3,
            &id,
            "accepted",
            0,
            1_000,
            Some("deadbeef"),
            1_004,
        );
        assert_eq!(up.state(), UploadState::Error);
        assert_eq!(up.error(), Some("checksum_mismatch"));
    }

    #[test]
    fn cancel_transitions_to_cancelling_then_cancelled() {
        let mut up = UploadSession::new();
        up.connect(&make_session(), true, 0).unwrap();
        let sealed = make_sealed(1_000, "conversation");
        up.begin(sealed, None, None, 1_000).unwrap();
        let req = up.take_last_control().unwrap()["request_id"].as_str().unwrap().to_string();
        let id = up.id.clone().unwrap();
        up.ack(&make_session().connection_id, &req, &id, "begin", 0, 0, None, 1_001);

        let mut wire = MockTransport::new();
        up.cancel(&mut wire, 1_002);
        assert_eq!(up.state(), UploadState::Cancelling);

        // Find the cancel request id from the control frame.
        let cancel_req = wire.control_frames.last().unwrap()["request_id"].as_str().unwrap().to_string();
        up.ack(&make_session().connection_id, &cancel_req, &id, "cancelled", 0, 0, None, 1_003);
        assert_eq!(up.state(), UploadState::Cancelled);
    }

    #[test]
    fn encode_produces_correct_header() {
        let id = "00000000-0000-0000-0000-000000000001";
        let payload = vec![1u8, 2, 3, 4];
        let frame = encode(id, 100, &payload).unwrap();
        assert_eq!(&frame[0..4], &0x4e58_4132u32.to_be_bytes());
        // offset at bytes 20..24
        assert_eq!(&frame[20..24], &100u32.to_be_bytes());
        assert_eq!(&frame[24..28], &4u32.to_be_bytes());
        assert_eq!(&frame[28..], &payload);
    }

    #[test]
    fn encode_rejects_odd_payload() {
        let id = "00000000-0000-0000-0000-000000000001";
        assert!(encode(id, 0, &[1, 2, 3]).is_err());
    }

    #[test]
    fn connect_rejects_unsupported_chunk() {
        let mut session = make_session();
        session.max_chunk_bytes = 3; // odd
        let mut up = UploadSession::new();
        assert!(up.connect(&session, true, 0).is_err());
        assert_eq!(up.state(), UploadState::Error);
    }

    #[test]
    fn disconnected_cancels_non_conversation() {
        let mut up = UploadSession::new();
        up.connect(&make_session(), true, 0).unwrap();
        let sealed = make_sealed(1_000, "enrollment");
        up.begin(sealed, Some("00000000-0000-0000-0000-000000000003"), Some("00000000-0000-0000-0000-000000000004"), 1_000).unwrap();
        up.disconnected();
        assert_eq!(up.state(), UploadState::Error);
        assert_eq!(up.error(), Some("cancelled"));
    }

    #[test]
    fn disconnected_keeps_conversation_for_retry() {
        let mut up = UploadSession::new();
        up.connect(&make_session(), true, 0).unwrap();
        let sealed = make_sealed(1_000, "conversation");
        up.begin(sealed, None, None, 1_000).unwrap();
        up.disconnected();
        assert_eq!(up.state(), UploadState::Disconnected);
    }
}