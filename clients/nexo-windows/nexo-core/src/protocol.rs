//! Protocol data models and `outdoor` negotiation.
//!
//! Ported from `android/.../network/ProtocolModels.kt` and
//! `android/.../network/OutdoorProtocol.kt`. Server and protocol are read-only
//! from this client's perspective — the wire format must match the Android
//! client byte-for-byte at the JSON level.

use serde::{Deserialize, Serialize};

/// Capability flags the server requires before a v2 session is established.
pub const CAP_UTTERANCE_UPLOAD_V2: &str = "utterance_upload_v2";
pub const CAP_EXCLUSIVE_AUDIO_V1: &str = "exclusive_audio_v1";

/// Canonical protocol version this client speaks.
pub const PROTOCOL_VERSION: &str = "1.0";

// ── Audio spec ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSpec {
    #[serde(rename = "format")]
    pub format: String,
    #[serde(rename = "sample_rate")]
    pub sample_rate: u32,
    #[serde(rename = "channels")]
    pub channels: u16,
    #[serde(rename = "bit_depth")]
    pub bit_depth: u16,
}

impl Default for AudioSpec {
    fn default() -> Self {
        Self {
            format: "audio/pcm".into(),
            sample_rate: 16_000,
            channels: 1,
            bit_depth: 16,
        }
    }
}

impl AudioSpec {
    /// Voice capture spec: 16 kHz mono PCM16.
    pub fn voice_capture() -> Self {
        Self::default()
    }
    /// Voice playback spec: 24 kHz mono PCM16 (server output rate).
    pub fn voice_playback() -> Self {
        Self {
            format: "audio/pcm".into(),
            sample_rate: 24_000,
            channels: 1,
            bit_depth: 16,
        }
    }
}

// ── Client → server ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ClientHello {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "protocol_version")]
    pub protocol_version: String,
    #[serde(rename = "device_id")]
    pub device_id: String,
    #[serde(rename = "audio_spec")]
    pub audio_spec: AudioSpec,
    #[serde(rename = "capabilities")]
    pub capabilities: Vec<String>,
}

impl ClientHello {
    pub fn new(device_id: impl Into<String>) -> Self {
        Self {
            msg_type: "client_hello".into(),
            protocol_version: PROTOCOL_VERSION.into(),
            device_id: device_id.into(),
            audio_spec: AudioSpec::voice_capture(),
            capabilities: vec![
                CAP_UTTERANCE_UPLOAD_V2.into(),
                CAP_EXCLUSIVE_AUDIO_V1.into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceStart {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "timestamp")]
    pub timestamp: i64,
}

impl VoiceStart {
    pub fn now() -> Self {
        Self {
            msg_type: "voice_start".into(),
            timestamp: chrono_now_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VoiceEnd {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "timestamp")]
    pub timestamp: i64,
}

impl VoiceEnd {
    pub fn now() -> Self {
        Self {
            msg_type: "voice_end".into(),
            timestamp: chrono_now_millis(),
        }
    }
}

// ── Server → client ────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ServerHello {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "session_id")]
    pub session_id: String,
    #[serde(rename = "agent_name")]
    pub agent_name: String,
    #[serde(rename = "status")]
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StateChange {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "state")]
    pub state: String,
    #[serde(rename = "message")]
    pub message: Option<String>,
    #[serde(rename = "expression")]
    pub expression: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VisualTelemetry {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "amplitude")]
    pub amplitude: f32,
    #[serde(rename = "frequency_bands")]
    pub frequency_bands: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptionMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "role")]
    pub role: String,
    #[serde(rename = "text")]
    pub text: String,
    #[serde(rename = "is_final")]
    pub is_final: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PongMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(rename = "timestamp")]
    pub timestamp: i64,
}

// ── Outdoor negotiation (port of OutdoorProtocol.kt) ────────────────────

/// The negotiated v2 session parameters the server grants after `server_hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutdoorSession {
    pub connection_id: String,
    pub server_instance_id: String,
    pub output_epoch: u64,
    pub max_chunk_bytes: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum NegotiateError {
    #[error("server_hello is missing required `capabilities` array")]
    MissingCapabilities,
    #[error("server did not negotiate both {CAP_UTTERANCE_UPLOAD_V2} and {CAP_EXCLUSIVE_AUDIO_V1}")]
    UnsupportedCapabilities,
    #[error("server protocol_version must be 2.0")]
    WrongProtocolVersion,
    #[error("conversation_paused must be true for an exclusive audio session")]
    NotPaused,
    #[error("invalid or missing connection_id")]
    InvalidConnectionId,
    #[error("invalid or missing server_instance_id")]
    InvalidServerInstanceId,
    #[error("invalid output_epoch")]
    InvalidOutputEpoch,
    #[error("missing `limits` object")]
    MissingLimits,
    #[error("limits.max_samples must be exactly 320000")]
    BadMaxSamples,
    #[error("limits.retry_ttl_ms must be exactly 30000")]
    BadRetryTtl,
    #[error("limits.owner_lease_ms must be exactly 15000")]
    BadOwnerLease,
    #[error("limits.max_chunk_bytes out of range (must be 2..=16384, even)")]
    BadMaxChunkBytes,
}

/// Negotiate an [`OutdoorSession`] from a `server_hello` JSON value.
///
/// Mirrors `OutdoorProtocol.negotiate()` — every rejection in the Kotlin
/// version maps to a distinct [`NegotiateError`] variant so callers can log
/// *why* negotiation failed rather than a generic "unsupported".
pub fn negotiate(json: &serde_json::Value) -> Result<OutdoorSession, NegotiateError> {
    let capabilities = json
        .get("capabilities")
        .and_then(|v| v.as_array())
        .ok_or(NegotiateError::MissingCapabilities)?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect::<Vec<_>>();

    if !(capabilities.iter().any(|c| c == CAP_UTTERANCE_UPLOAD_V2)
        && capabilities.iter().any(|c| c == CAP_EXCLUSIVE_AUDIO_V1))
    {
        return Err(NegotiateError::UnsupportedCapabilities);
    }

    if json.get("protocol_version").and_then(|v| v.as_str()) != Some("2.0") {
        return Err(NegotiateError::WrongProtocolVersion);
    }

    let paused = json
        .get("conversation_paused")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !paused {
        return Err(NegotiateError::NotPaused);
    }

    let connection_id = uuid_field(json, "connection_id").ok_or(NegotiateError::InvalidConnectionId)?;
    let server_instance_id =
        uuid_field(json, "server_instance_id").ok_or(NegotiateError::InvalidServerInstanceId)?;
    let output_epoch =
        u64_field(json, "output_epoch", 0, u64::MAX).ok_or(NegotiateError::InvalidOutputEpoch)?;

    let limits = json.get("limits").and_then(|v| v.as_object()).ok_or(NegotiateError::MissingLimits)?;

    if int_field(limits, "max_samples", 320_000, 320_000).is_none() {
        return Err(NegotiateError::BadMaxSamples);
    }
    if int_field(limits, "retry_ttl_ms", 30_000, 30_000).is_none() {
        return Err(NegotiateError::BadRetryTtl);
    }
    if int_field(limits, "owner_lease_ms", 15_000, 15_000).is_none() {
        return Err(NegotiateError::BadOwnerLease);
    }

    let max_chunk = int_field(limits, "max_chunk_bytes", 2, 16_384).ok_or(NegotiateError::BadMaxChunkBytes)?;
    if max_chunk % 2 != 0 {
        return Err(NegotiateError::BadMaxChunkBytes);
    }

    Ok(OutdoorSession {
        connection_id,
        server_instance_id,
        output_epoch,
        max_chunk_bytes: max_chunk as u32,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn uuid_field(json: &serde_json::Value, key: &str) -> Option<String> {
    let s = json.get(key)?.as_str()?;
    // Must round-trip as a canonical UUID (port of UploadSession.uuid).
    uuid::Uuid::parse_str(s).ok()?;
    if uuid::Uuid::parse_str(s).ok()?.to_string() == s {
        Some(s.to_string())
    } else {
        None
    }
}

fn int_field(
    json: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    minimum: u64,
    maximum: u64,
) -> Option<u64> {
    let v = json.get(key)?;
    let n = v.as_u64()?;
    if n >= minimum && n <= maximum {
        Some(n)
    } else {
        None
    }
}

fn u64_field(json: &serde_json::Value, key: &str, minimum: u64, maximum: u64) -> Option<u64> {
    let v = json.get(key)?;
    let n = v.as_u64()?;
    if n >= minimum && n <= maximum {
        Some(n)
    } else {
       None
    }
}

/// Current UNIX timestamp in milliseconds. Pure helper so tests stay
/// deterministic when they need a fixed clock.
pub fn chrono_now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn client_hello_matches_android_wire_shape() {
        let hello = ClientHello::new("test-device");
        let v = serde_json::to_value(&hello).unwrap();
        assert_eq!(v["type"], "client_hello");
        assert_eq!(v["protocol_version"], "1.0");
        assert_eq!(v["device_id"], "test-device");
        assert_eq!(v["audio_spec"]["format"], "audio/pcm");
        assert_eq!(v["audio_spec"]["sample_rate"], 16000);
        assert_eq!(v["audio_spec"]["channels"], 1);
        assert_eq!(v["audio_spec"]["bit_depth"], 16);
        let caps = v["capabilities"].as_array().unwrap();
        assert!(caps.iter().any(|c| c == "utterance_upload_v2"));
        assert!(caps.iter().any(|c| c == "exclusive_audio_v1"));
    }

    fn valid_server_hello() -> serde_json::Value {
        json!({
            "type": "server_hello",
            "session_id": "sess-1",
            "agent_name": "Nexo",
            "status": "ready",
            "protocol_version": "2.0",
            "conversation_paused": true,
            "connection_id": "00000000-0000-0000-0000-000000000001",
            "server_instance_id": "00000000-0000-0000-0000-000000000002",
            "output_epoch": 7,
            "capabilities": ["utterance_upload_v2", "exclusive_audio_v1"],
            "limits": {
                "max_samples": 320000,
                "retry_ttl_ms": 30000,
                "owner_lease_ms": 15000,
                "max_chunk_bytes": 8192
            }
        })
    }

    #[test]
    fn negotiate_accepts_valid_server_hello() {
        let session = negotiate(&valid_server_hello()).expect("should negotiate");
        assert_eq!(session.connection_id, "00000000-0000-0000-0000-000000000001");
        assert_eq!(session.output_epoch, 7);
        assert_eq!(session.max_chunk_bytes, 8192);
    }

    #[test]
    fn negotiate_rejects_missing_capabilities() {
        let mut v = valid_server_hello();
        v.as_object_mut().unwrap().remove("capabilities");
        assert!(matches!(negotiate(&v), Err(NegotiateError::MissingCapabilities)));
    }

    #[test]
    fn negotiate_rejects_wrong_protocol_version() {
        let mut v = valid_server_hello();
        v["protocol_version"] = json!("1.0");
        assert!(matches!(negotiate(&v), Err(NegotiateError::WrongProtocolVersion)));
    }

    #[test]
    fn negotiate_rejects_unpaused_conversation() {
        let mut v = valid_server_hello();
        v["conversation_paused"] = json!(false);
        assert!(matches!(negotiate(&v), Err(NegotiateError::NotPaused)));
    }

    #[test]
    fn negotiate_rejects_invalid_uuid() {
        let mut v = valid_server_hello();
        v["connection_id"] = json!("not-a-uuid");
        assert!(matches!(negotiate(&v), Err(NegotiateError::InvalidConnectionId)));
    }

    #[test]
    fn negotiate_rejects_bad_max_chunk() {
        let mut v = valid_server_hello();
        v["limits"]["max_chunk_bytes"] = json!(16385); // out of range
        assert!(matches!(negotiate(&v), Err(NegotiateError::BadMaxChunkBytes)));
    }

    #[test]
    fn negotiate_rejects_odd_max_chunk() {
        let mut v = valid_server_hello();
        v["limits"]["max_chunk_bytes"] = json!(8191); // odd
        assert!(matches!(negotiate(&v), Err(NegotiateError::BadMaxChunkBytes)));
    }

    #[test]
    fn audio_spec_defaults_match_android() {
        let spec = AudioSpec::default();
        assert_eq!(spec.format, "audio/pcm");
        assert_eq!(spec.sample_rate, 16000);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bit_depth, 16);
    }

    #[test]
    fn server_hello_roundtrips() {
        let v = valid_server_hello();
        let hello: ServerHello = serde_json::from_value(v).expect("parse server_hello");
        assert_eq!(hello.session_id, "sess-1");
        assert_eq!(hello.agent_name, "Nexo");
        assert_eq!(hello.status, "ready");
    }

    #[test]
    fn transcription_roundtrips() {
        let v = json!({
            "type": "transcription",
            "role": "assistant",
            "text": "Halo, apa kabar?",
            "is_final": true
        });
        let msg: TranscriptionMessage = serde_json::from_value(v).unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.text, "Halo, apa kabar?");
        assert!(msg.is_final);
    }
}