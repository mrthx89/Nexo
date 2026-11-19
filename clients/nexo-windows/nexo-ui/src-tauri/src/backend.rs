//! Backend boundary for Nexo Windows UI.
//!
//! This module defines the interface the Tauri/UI layer consumes. Phase 2 ships a
//! `MockBackend` that drives the same state/telemetry/transcript shape the real
//! `nexo-core`-backed implementation will emit. Real audio/VAD/WSS logic lives in
//! `nexo-core` (Phase 1) and is intentionally out of scope here.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NexoState {
    Connecting,
    Idle,
    Listening,
    Processing,
    Answering,
    Paused,
}

impl NexoState {
    pub fn label(&self) -> &'static str {
        match self {
            NexoState::Connecting => "Menghubungkan",
            NexoState::Idle => "Siaga",
            NexoState::Listening => "Menyimak",
            NexoState::Processing => "Memproses",
            NexoState::Answering => "Menjawab",
            NexoState::Paused => "Paused",
        }
    }
    pub fn expression(&self) -> &'static str {
        match self {
            NexoState::Connecting => "bingung",
            NexoState::Idle => "netral",
            NexoState::Listening => "netral",
            NexoState::Processing => "bingung",
            NexoState::Answering => "senang",
            NexoState::Paused => "mengantuk",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualTelemetry {
    pub amplitude: f32,
    pub frequency_bands: Vec<f32>,
}

impl Default for VisualTelemetry {
    fn default() -> Self {
        Self { amplitude: 0.0, frequency_bands: vec![0.0, 0.0, 0.0, 0.0] }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub id: String,
    pub role: String,
    pub text: String,
    pub timestamp: i64,
    pub r#final: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSnapshot {
    pub state: NexoState,
    pub expression: String,
    pub telemetry: VisualTelemetry,
    pub transcript: Vec<TranscriptEntry>,
    pub mic_enabled: bool,
    pub connected: bool,
    pub status_text: String,
    pub server_url: String,
    pub device_id: String,
    pub has_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSettings {
    pub server_url: String,
    pub token: String,
    pub device_id: Option<String>,
}

/// Trait boundary: real nexo-core integration must satisfy this surface.
pub trait NexoBackendEngine: Send + Sync {
    fn snapshot(&self) -> BackendSnapshot;
    fn set_mic_enabled(&self, enabled: bool) -> BackendSnapshot;
    fn toggle_pause(&self) -> BackendSnapshot;
    fn update_settings(&self, settings: BackendSettings) -> BackendSnapshot;
    fn tick(&self) -> Option<BackendSnapshot>;
}

#[derive(Debug)]
struct MockInner {
    state_index: usize,
    tick: u64,
    auto_cycle: bool,
    mic_enabled: bool,
    connected: bool,
    server_url: String,
    device_id: String,
    has_token: bool,
    transcript: Vec<TranscriptEntry>,
}

const STATES: [NexoState; 6] = [
    NexoState::Connecting,
    NexoState::Idle,
    NexoState::Listening,
    NexoState::Processing,
    NexoState::Answering,
    NexoState::Paused,
];

#[derive(Debug, Clone)]
pub struct MockBackend {
    inner: Arc<Mutex<MockInner>>,
}

impl Default for MockBackend {
    fn default() -> Self {
        let now = chrono_like_now();
        Self {
            inner: Arc::new(Mutex::new(MockInner {
                state_index: 0,
                tick: 0,
                auto_cycle: true,
                mic_enabled: true,
                connected: false,
                server_url: "wss://nexo.yhdev.my.id/ws/appliance".to_string(),
                device_id: "nexo-windows-mock".to_string(),
                has_token: false,
                transcript: vec![
                    TranscriptEntry {
                        id: "t1".into(),
                        role: "assistant".into(),
                        text: "Halo, saya Nexo. Mode simulasi UI aktif (mock backend).".into(),
                        timestamp: now - 15000,
                        r#final: true,
                    },
                    TranscriptEntry {
                        id: "t2".into(),
                        role: "user".into(),
                        text: "Nexo, tampilkan status kiosk.".into(),
                        timestamp: now - 9000,
                        r#final: true,
                    },
                    TranscriptEntry {
                        id: "t3".into(),
                        role: "assistant".into(),
                        text: "Kiosk siap. Capsule, panel percakapan, dan state machine tersambung via interface backend.".into(),
                        timestamp: now - 4000,
                        r#final: true,
                    },
                ],
            })),
        }
    }
}

fn chrono_like_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn clamp01(v: f32) -> f32 { v.max(0.0).min(1.0) }

impl MockBackend {
    fn make_snapshot(inner: &MockInner) -> BackendSnapshot {
        let state = STATES[inner.state_index].clone();
        let base_amp: f32 = match state {
            NexoState::Listening => 0.35,
            NexoState::Answering => 0.55,
            NexoState::Processing => 0.18,
            _ => 0.06,
        };
        let phase = inner.tick as f32 * 0.9;
        let amplitude = clamp01(base_amp + phase.sin() * 0.18);
        let bands = (0..4)
            .map(|i| {
                let fi = i as f32;
                clamp01(base_amp * (0.5 + 0.15 * fi) + (phase + fi * 0.8).sin().abs() * 0.35)
            })
            .collect::<Vec<_>>();
        let connected = !matches!(state, NexoState::Connecting);
        let status_text = if connected {
            format!("Status: {}", state.label())
        } else {
            "Menghubungkan ke layanan (simulasi)…".to_string()
        };
        BackendSnapshot {
            expression: state.expression().to_string(),
            state,
            telemetry: VisualTelemetry { amplitude, frequency_bands: bands },
            transcript: inner.transcript.clone(),
            mic_enabled: inner.mic_enabled,
            connected,
            status_text,
            server_url: inner.server_url.clone(),
            device_id: inner.device_id.clone(),
            has_token: inner.has_token,
        }
    }

    fn push_transcript(inner: &mut MockInner, role: &str, text: &str) {
        let id = format!("m{}-{}", chrono_like_now(), inner.tick);
        inner.transcript.push(TranscriptEntry {
            id,
            role: role.to_string(),
            text: text.to_string(),
            timestamp: chrono_like_now(),
            r#final: true,
        });
        if inner.transcript.len() > 50 {
            let drop_n = inner.transcript.len() - 50;
            inner.transcript.drain(0..drop_n);
        }
    }
}

impl NexoBackendEngine for MockBackend {
    fn snapshot(&self) -> BackendSnapshot {
        let inner = self.inner.lock().expect("mock backend mutex");
        Self::make_snapshot(&inner)
    }

    fn set_mic_enabled(&self, enabled: bool) -> BackendSnapshot {
        let mut inner = self.inner.lock().expect("mock backend mutex");
        inner.mic_enabled = enabled;
        if !enabled {
            inner.auto_cycle = false;
            inner.state_index = 5; // PAUSED
        } else if inner.state_index == 5 {
            inner.auto_cycle = true;
            inner.state_index = 1; // IDLE
        }
        Self::make_snapshot(&inner)
    }

    fn toggle_pause(&self) -> BackendSnapshot {
        let enabled = {
            let inner = self.inner.lock().expect("mock backend mutex");
            !(inner.mic_enabled && inner.state_index != 5)
        };
        self.set_mic_enabled(enabled)
    }

    fn update_settings(&self, settings: BackendSettings) -> BackendSnapshot {
        let mut inner = self.inner.lock().expect("mock backend mutex");
        if !settings.server_url.trim().is_empty() {
            inner.server_url = settings.server_url;
        }
        if let Some(did) = settings.device_id {
            if !did.trim().is_empty() {
                inner.device_id = did;
            }
        }
        inner.has_token = !settings.token.is_empty() && settings.token != "[REDACTED]";
        Self::push_transcript(&mut inner, "system", "Pengaturan diperbarui (token/URL) — diterapkan ke backend saat integrasi nyata.");
        Self::make_snapshot(&inner)
    }

    fn tick(&self) -> Option<BackendSnapshot> {
        let mut inner = self.inner.lock().expect("mock backend mutex");
        inner.tick += 1;
        if inner.auto_cycle && inner.tick % 5 == 0 {
            inner.state_index = (inner.state_index + 1) % STATES.len();
        }
        let idx = inner.state_index;
        let t = inner.tick;
        if idx == 2 && t % 10 == 0 {
            Self::push_transcript(&mut inner, "user", "Contoh input suara (simulasi).");
        }
        if idx == 4 && t % 10 == 5 {
            Self::push_transcript(&mut inner, "assistant", "Respons simulasi: state machine UI aktif dan ekspresi mengikuti status.");
        }
        Some(Self::make_snapshot(&inner))
    }
}
