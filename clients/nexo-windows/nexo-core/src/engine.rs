//! Full headless Nexo v2 appliance engine mirroring Android `NexoKioskService.kt`.
//!
//! Coordinates WASAPI capture/resampling, Silero VAD, `SpeechSegmenter`, `UtteranceBuffer`,
//! `UploadSession`, `HalfDuplexGate`, 24 kHz TTS `PlaybackSession`, `PrivateDiagnosticClip`,
//! voiceprint enrollment leases, and always-on auto-recovery.

use crate::audio::{self, CaptureConfig, CaptureSession, PlaybackSession};
use crate::buffer::{prepare_vad_window, CapturePurpose, UtteranceBuffer};
use crate::config::{app_data_dir, validate_wss_url};
use crate::credential_store::DeviceCredentialStore;
use crate::diagnostic::PrivateDiagnosticClip;
use crate::half_duplex::{GateState, HalfDuplexGate};
use crate::protocol::{self, chrono_now_millis, ClientHello, OutdoorSession};
use crate::segmenter::{Boundary, SpeechSegmenter};
use crate::upload::{Sealed, Transport, UploadSession, UploadState};
use crate::vad::SileroVad;
use crate::ws::{Incoming, WsConnection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientState {
    Disconnected,
    Idle,
    Listening,
    Thinking,
    Speaking,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSessionSnapshot {
    pub state: String,
    pub message: String,
    pub ready: bool,
    pub sample_count: usize,
    pub required_samples: usize,
    pub diagnostic: bool,
    pub diagnostic_ready: bool,
    pub is_enrolled: Option<bool>,
    pub voiceprint_available: Option<bool>,
    pub voiceprint_message: Option<String>,
}

impl Default for RecordingSessionSnapshot {
    fn default() -> Self {
        Self {
            state: "CLOSED".into(),
            message: "Percakapan dijeda".into(),
            ready: false,
            sample_count: 0,
            required_samples: 0,
            diagnostic: false,
            diagnostic_ready: false,
            is_enrolled: None,
            voiceprint_available: None,
            voiceprint_message: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSnapshot {
    pub state: ClientState,
    pub message: String,
    pub voice_enabled: bool,
    pub automatic_mode: bool,
    pub vad_available: bool,
    pub expression: Option<String>,
    pub amplitude: f32,
    pub can_finish: bool,
    pub last_reply: String,
    pub recording: RecordingSessionSnapshot,
    pub server_url: String,
    pub device_id: String,
    pub has_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum EngineEvent {
    Snapshot(EngineSnapshot),
    Transcription {
        role: String,
        text: String,
        is_final: bool,
    },
    Telemetry {
        amplitude: f32,
    },
}

#[derive(Debug)]
pub enum EngineCommand {
    StartVoice,
    StopVoice,
    FinishUtterance,
    StartManualUtterance,
    SetAutomatic(bool),
    UpdateConfig {
        server_url: String,
        token: Option<String>,
        vad_model: Option<PathBuf>,
    },
    ClearToken,
    OpenRecordingSheet {
        diagnostic_only: bool,
    },
    CloseRecordingSheet {
        resume: bool,
    },
    StartEnrollmentOrDiagnostic,
    SaveEnrollment,
    UnenrollVoiceprint,
    PlayDiagnostic,
    DeleteDiagnostic,
    Shutdown,
}

#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::UnboundedSender<EngineCommand>,
    events: broadcast::Sender<EngineEvent>,
    snapshot: Arc<RwLock<EngineSnapshot>>,
}

impl EngineHandle {
    pub fn send(&self, cmd: EngineCommand) -> anyhow::Result<()> {
        self.tx
            .send(cmd)
            .map_err(|_| anyhow::anyhow!("engine stopped"))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    pub async fn snapshot(&self) -> EngineSnapshot {
        self.snapshot.read().await.clone()
    }
}

pub struct EngineConfig {
    pub server_url: String,
    pub device_id: String,
    pub token: Option<String>,
    pub vad_model: Option<PathBuf>,
}

pub fn spawn_engine(initial: EngineConfig) -> EngineHandle {
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (evt_tx, _) = broadcast::channel(128);
    let has_token = initial
        .token
        .as_deref()
        .map(|t| !t.trim().is_empty() && t != crate::config::TOKEN_PLACEHOLDER)
        .unwrap_or(false);
    let vad_available = initial
        .vad_model
        .as_ref()
        .map(|p| p.is_file())
        .unwrap_or(false);

    let snap = EngineSnapshot {
        state: ClientState::Idle,
        message: "Dijeda, tekan Mulai mikrofon untuk berbicara".into(),
        voice_enabled: false,
        automatic_mode: vad_available,
        vad_available,
        expression: Some("netral".into()),
        amplitude: 0.0,
        can_finish: false,
        last_reply: String::new(),
        recording: RecordingSessionSnapshot::default(),
        server_url: initial.server_url.clone(),
        device_id: initial.device_id.clone(),
        has_token,
    };
    let snapshot = Arc::new(RwLock::new(snap));

    let evt_clone = evt_tx.clone();
    let snap_clone = Arc::clone(&snapshot);
    tokio::spawn(async move {
        let mut runner = EngineRunner::new(initial, cmd_rx, evt_clone, snap_clone);
        runner.run().await;
    });

    EngineHandle {
        tx: cmd_tx,
        events: evt_tx,
        snapshot,
    }
}

struct WsQueueTransport {
    queued_out_json: Vec<Value>,
    queued_out_bin: Vec<Vec<u8>>,
}

impl WsQueueTransport {
    fn new() -> Self {
        Self {
            queued_out_json: Vec::new(),
            queued_out_bin: Vec::new(),
        }
    }
}

impl Transport for WsQueueTransport {
    fn control(&mut self, value: &Value) -> bool {
        self.queued_out_json.push(value.clone());
        true
    }
    fn binary(&mut self, value: &[u8]) -> bool {
        self.queued_out_bin.push(value.to_vec());
        true
    }
    fn queued_bytes(&self) -> u64 {
        0
    }
}

enum InternalMsg {
    CaptureSealed(Sealed),
    CaptureLevel(f32, bool),
    CaptureError(String),
    PlaybackDrained(u64),
}

struct SharedCaptureControl {
    buffer: Mutex<UtteranceBuffer>,
    purpose: Mutex<CapturePurpose>,
    finish_now: std::sync::atomic::AtomicBool,
    manual_start_now: std::sync::atomic::AtomicBool,
}

struct EngineRunner {
    server_url: String,
    device_id: String,
    token: Option<String>,
    vad_model: Option<PathBuf>,
    cred_store: DeviceCredentialStore,

    cmd_rx: mpsc::UnboundedReceiver<EngineCommand>,
    evt_tx: broadcast::Sender<EngineEvent>,
    snapshot: Arc<RwLock<EngineSnapshot>>,
    internal_tx: mpsc::UnboundedSender<InternalMsg>,
    internal_rx: mpsc::UnboundedReceiver<InternalMsg>,

    conn: Option<WsConnection>,
    session: Option<OutdoorSession>,
    gate: HalfDuplexGate,
    upload: UploadSession,
    transport: WsQueueTransport,
    playback: Option<Arc<PlaybackSession>>,
    capture_session: Option<CaptureSession>,
    capture_shared: Option<Arc<SharedCaptureControl>>,
    diagnostic: Option<PrivateDiagnosticClip>,

    keep_voice_alive: bool,
    auto_restart_at: Option<Instant>,
    reconnect_attempt: u32,
    automatic_mode: bool,
    vad_available: bool,
    resumed_normal: bool,
    resume_request_id: Option<String>,
    pending_utterance_id: Option<String>,
    processing_deadline: Option<Instant>,
    playback_epoch: u64,
    playback_drained_at: Option<Instant>,
    sleep_expression: Option<String>,
    current_state: ClientState,
    current_message: String,
    current_expression: Option<String>,
    last_reply: String,

    // Dedicated recording sheet state
    sheet_owner: Option<String>,
    lease_id: Option<String>,
    lease_renew_at: Option<Instant>,
    acquire_request_id: Option<String>,
    release_request_id: Option<String>,
    enrollment_id: Option<String>,
    enrollment_request_id: Option<String>,
    enrollment_operation: Option<String>,
    enrollment_samples: Vec<String>,
    resume_after_release: bool,
    recording: RecordingSessionSnapshot,
}

impl EngineRunner {
    fn new(
        initial: EngineConfig,
        cmd_rx: mpsc::UnboundedReceiver<EngineCommand>,
        evt_tx: broadcast::Sender<EngineEvent>,
        snapshot: Arc<RwLock<EngineSnapshot>>,
    ) -> Self {
        let (internal_tx, internal_rx) = mpsc::unbounded_channel();
        let vad_available = initial
            .vad_model
            .as_ref()
            .map(|p| p.is_file())
            .unwrap_or(false);
        let playback = PlaybackSession::start_default().ok().map(Arc::new);
        let diag_dir = app_data_dir().join("audio-diagnostic");
        let diagnostic = PrivateDiagnosticClip::new(&diag_dir).ok();

        Self {
            server_url: initial.server_url,
            device_id: initial.device_id,
            token: initial.token,
            vad_model: initial.vad_model,
            cred_store: DeviceCredentialStore::default_store(),
            cmd_rx,
            evt_tx,
            snapshot,
            internal_tx,
            internal_rx,
            conn: None,
            session: None,
            gate: HalfDuplexGate::new(),
            upload: UploadSession::new(),
            transport: WsQueueTransport::new(),
            playback,
            capture_session: None,
            capture_shared: None,
            diagnostic,
            keep_voice_alive: false,
            auto_restart_at: None,
            reconnect_attempt: 0,
            automatic_mode: vad_available,
            vad_available,
            resumed_normal: false,
            resume_request_id: None,
            pending_utterance_id: None,
            processing_deadline: None,
            playback_epoch: 0,
            playback_drained_at: None,
            sleep_expression: None,
            current_state: ClientState::Idle,
            current_message: "Dijeda, tekan Mulai mikrofon untuk berbicara".into(),
            current_expression: Some("netral".into()),
            last_reply: String::new(),
            sheet_owner: None,
            lease_id: None,
            lease_renew_at: None,
            acquire_request_id: None,
            release_request_id: None,
            enrollment_id: None,
            enrollment_request_id: None,
            enrollment_operation: None,
            enrollment_samples: Vec::new(),
            resume_after_release: false,
            recording: RecordingSessionSnapshot::default(),
        }
    }

    async fn run(&mut self) {
        let mut tick = tokio::time::interval(Duration::from_millis(80));
        loop {
            tokio::select! {
                cmd = self.cmd_rx.recv() => {
                    match cmd {
                        Some(EngineCommand::Shutdown) | None => {
                            self.stop_voice_internal().await;
                            break;
                        }
                        Some(c) => self.handle_command(c).await,
                    }
                }
                msg = self.internal_rx.recv() => {
                    if let Some(m) = msg {
                        self.handle_internal(m).await;
                    }
                }
                ws_msg = async {
                    match self.conn.as_mut() {
                        Some(c) => Some(c.next_message().await),
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(res) = ws_msg {
                        match res {
                            Ok(Some(inc)) => self.handle_incoming(inc).await,
                            Ok(None) => self.handle_disconnect("Koneksi server ditutup").await,
                            Err(e) => self.handle_disconnect(&format!("Koneksi terputus: {e}")).await,
                        }
                    }
                }
                _ = tick.tick() => {
                    self.on_tick().await;
                }
            }
        }
    }

    async fn handle_command(&mut self, cmd: EngineCommand) {
        match cmd {
            EngineCommand::StartVoice => {
                self.keep_voice_alive = true;
                self.auto_restart_at = None;
                self.reconnect_attempt = 0;
                self.ensure_connected_and_resume().await;
            }
            EngineCommand::StopVoice => {
                self.stop_voice_internal().await;
            }
            EngineCommand::FinishUtterance => {
                if let Some(sh) = &self.capture_shared {
                    sh.finish_now.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            EngineCommand::StartManualUtterance => {
                if let Some(sh) = &self.capture_shared {
                    sh.manual_start_now.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            EngineCommand::SetAutomatic(val) => {
                if !val || self.vad_available {
                    self.automatic_mode = val;
                    if self.capture_session.is_some() {
                        self.stop_capture();
                        if self.resumed_normal {
                            let _ = self.start_capture(CapturePurpose::Conversation);
                        }
                    }
                    self.emit_snapshot(0.0).await;
                }
            }
            EngineCommand::UpdateConfig { server_url, token, vad_model } => {
                match validate_wss_url(&server_url) {
                    Ok(valid_url) => {
                        self.server_url = valid_url.clone();
                        let _ = self.cred_store.save_server_url(&valid_url);
                        if let Some(t) = token.filter(|s| !s.trim().is_empty()) {
                            if self.cred_store.save_token(&valid_url, &self.device_id, &t).is_ok() {
                                self.token = Some(t);
                            }
                        }
                        if let Some(m) = vad_model {
                            self.vad_available = m.is_file();
                            self.vad_model = Some(m);
                        }
                        self.stop_voice_internal().await;
                        self.publish_state(ClientState::Idle, "Pengaturan tersimpan. Tekan Mulai mikrofon.", None).await;
                    }
                    Err(e) => {
                        self.publish_state(ClientState::Error, &format!("URL tidak valid: {e}"), None).await;
                    }
                }
            }
            EngineCommand::ClearToken => {
                let _ = self.cred_store.clear_token();
                self.token = None;
                self.stop_voice_internal().await;
                self.publish_state(ClientState::Idle, "Token perangkat dihapus. Isi token baru untuk terhubung.", None).await;
            }
            EngineCommand::OpenRecordingSheet { diagnostic_only } => {
                self.open_recording_sheet(diagnostic_only).await;
            }
            EngineCommand::CloseRecordingSheet { resume } => {
                self.close_recording_sheet(resume).await;
            }
            EngineCommand::StartEnrollmentOrDiagnostic => {
                self.start_sheet_capture().await;
            }
            EngineCommand::SaveEnrollment => {
                self.save_enrollment().await;
            }
            EngineCommand::UnenrollVoiceprint => {
                if self.recording.ready {
                    let req = Uuid::new_v4().to_string();
                    self.send_v2_control("voiceprint_unenroll", &req, json!({})).await;
                }
            }
            EngineCommand::PlayDiagnostic => {
                self.play_diagnostic().await;
            }
            EngineCommand::DeleteDiagnostic => {
                if let Some(diag) = self.diagnostic.as_mut() {
                    let _ = diag.delete();
                }
                self.recording.diagnostic_ready = false;
                if self.recording.diagnostic && self.recording.ready {
                    self.recording.state = "READY".into();
                    self.recording.message = "Klip lokal dihapus. Rekam ulang bila diperlukan.".into();
                }
                self.emit_snapshot(0.0).await;
            }
            EngineCommand::Shutdown => {}
        }
    }

    async fn ensure_connected_and_resume(&mut self) {
        let Some(tok) = self
            .token
            .as_deref()
            .filter(|t| !t.trim().is_empty() && *t != crate::config::TOKEN_PLACEHOLDER)
            .map(|s| s.to_string())
        else {
            self.keep_voice_alive = false;
            self.publish_state(
                ClientState::Error,
                "Token perangkat belum diatur. Buka Pengaturan untuk menyimpan bearer token.",
                None,
            )
            .await;
            return;
        };

        if self.conn.is_none() {
            self.publish_state(ClientState::Disconnected, "Menghubungkan ke server Nexo...", Some("bingung".into())).await;
            match WsConnection::connect_with_auth(&self.server_url, &tok, &self.device_id).await {
                Ok(mut c) => {
                    let hello = ClientHello::new(&self.device_id);
                    if let Ok(v) = serde_json::to_value(&hello) {
                        if c.send_json(&v).await.is_ok() {
                            self.conn = Some(c);
                            return;
                        }
                    }
                    self.schedule_auto_restart("Gagal mengirim client_hello").await;
                }
                Err(e) => {
                    self.schedule_auto_restart(&format!("Gagal terhubung: {e}")).await;
                }
            }
        } else if self.session.is_some() && !self.resumed_normal && self.sheet_owner.is_none() {
            self.request_resume().await;
        }
    }

    async fn request_resume(&mut self) {
        if self.session.is_none() || self.resume_request_id.is_some() {
            return;
        }
        let req = Uuid::new_v4().to_string();
        self.resume_request_id = Some(req.clone());
        self.send_v2_control("conversation_resume", &req, json!({})).await;
    }

    async fn stop_voice_internal(&mut self) {
        self.keep_voice_alive = false;
        self.auto_restart_at = None;
        self.resumed_normal = false;
        self.resume_request_id = None;
        self.pending_utterance_id = None;
        self.processing_deadline = None;
        self.stop_capture();
        if let Some(pb) = &self.playback {
            pb.reset_epoch();
        }
        self.gate.disconnected();
        self.upload.cancel(chrono_now_millis());
        if let Some(mut c) = self.conn.take() {
            let _ = c.close().await;
        }
        self.session = None;
        self.publish_state(ClientState::Idle, "Dijeda, tekan Mulai mikrofon untuk melanjutkan", self.sleep_expression.clone()).await;
    }

    async fn schedule_auto_restart(&mut self, msg: &str) {
        self.stop_capture();
        self.resumed_normal = false;
        self.resume_request_id = None;
        self.pending_utterance_id = None;
        self.processing_deadline = None;
        self.gate.disconnected();
        if let Some(mut c) = self.conn.take() {
            let _ = c.close().await;
        }
        self.session = None;
        if self.keep_voice_alive && self.sheet_owner.is_none() {
            self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
            let delay_ms = (1200 * (1u64 << self.reconnect_attempt.min(4))).min(15_000);
            self.auto_restart_at = Some(Instant::now() + Duration::from_millis(delay_ms));
            self.publish_state(
                ClientState::Disconnected,
                &format!("{msg} · menyambung ulang dalam {} dtk", delay_ms / 1000),
                Some(if self.reconnect_attempt <= 2 { "bingung".into() } else { "sedih".into() }),
            )
            .await;
        } else {
            self.publish_state(ClientState::Error, msg, self.sleep_expression.clone()).await;
        }
    }

    async fn handle_disconnect(&mut self, reason: &str) {
        if self.sheet_owner.is_some() {
            self.sheet_owner = None;
            self.lease_id = None;
            self.recording.state = "ERROR".into();
            self.recording.ready = false;
            self.recording.message = "Koneksi panel terputus. Tutup dan buka ulang panel.".into();
        }
        self.schedule_auto_restart(reason).await;
    }

    async fn handle_incoming(&mut self, inc: Incoming) {
        match inc {
            Incoming::Json(val) => {
                let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match msg_type {
                    "server_hello" => {
                        match protocol::negotiate(&val) {
                            Ok(sess) => {
                                self.reconnect_attempt = 0;
                                let _ = self.upload.connect(&sess, true, chrono_now_millis());
                                self.gate.connected();
                                self.session = Some(sess);
                                if self.sheet_owner.is_some() && self.acquire_request_id.is_none() {
                                    self.send_acquire().await;
                                } else if self.keep_voice_alive {
                                    self.request_resume().await;
                                } else {
                                    self.publish_state(ClientState::Idle, "Terhubung · tekan Mulai mikrofon", self.sleep_expression.clone()).await;
                                }
                            }
                            Err(e) => {
                                self.keep_voice_alive = false;
                                self.publish_state(ClientState::Error, &format!("Server tidak kompatibel: {e}"), None).await;
                            }
                        }
                    }
                    "conversation_resumed" => {
                        let req = val.get("request_id").and_then(|v| v.as_str());
                        if let Some(epoch) = val.get("output_epoch").and_then(|v| v.as_u64()) {
                            if let Some(s) = self.session.as_mut() {
                                s.output_epoch = epoch;
                            }
                        }
                        if req.is_some() && req == self.resume_request_id.as_deref() {
                            self.resume_request_id = None;
                            self.resumed_normal = true;
                            self.gate.microphone_ready(true);
                            self.gate.state_changed(GateState::Listening);
                            if let Err(e) = self.start_capture(CapturePurpose::Conversation) {
                                self.schedule_auto_restart(&format!("Mikrofon gagal dimulai: {e}")).await;
                            } else {
                                let label = if self.automatic_mode {
                                    "Siap, silakan berbicara"
                                } else {
                                    "Mode manual, tekan Mulai bicara"
                                };
                                self.publish_state(ClientState::Listening, label, self.sleep_expression.clone()).await;
                            }
                        }
                    }
                    "tts_start" => {
                        if !self.matches_output_epoch(&val) {
                            return;
                        }
                        self.processing_deadline = Some(Instant::now() + Duration::from_secs(30));
                        self.stop_capture();
                        self.gate.tts_started();
                        self.gate.state_changed(GateState::Speaking);
                        if let Some(pb) = &self.playback {
                            self.playback_epoch = pb.reset_epoch();
                        }
                        self.publish_state(ClientState::Speaking, "Menjawab...", self.current_expression.clone()).await;
                    }
                    "tts_end" => {
                        if !self.matches_output_epoch(&val) {
                            return;
                        }
                        let epoch = self.playback_epoch;
                        if let Some(pb) = self.playback.clone() {
                            let tx = self.internal_tx.clone();
                            tokio::task::spawn_blocking(move || {
                                let _ = pb.wait_drained(epoch, Duration::from_secs(10));
                                let _ = tx.send(InternalMsg::PlaybackDrained(epoch));
                            });
                        } else {
                            let _ = self.internal_tx.send(InternalMsg::PlaybackDrained(epoch));
                        }
                    }
                    "state_change" | "cognitive" => {
                        if let Some(expr) = val.get("expression").and_then(|v| v.as_str()) {
                            if expr == "sleeping" || expr == "tidur" {
                                self.sleep_expression = Some(expr.to_string());
                            } else {
                                self.sleep_expression = None;
                            }
                            self.current_expression = Some(expr.to_string());
                        }
                        if let Some(st) = val.get("state").and_then(|v| v.as_str()) {
                            self.processing_deadline = Some(Instant::now() + Duration::from_secs(30));
                            match st {
                                "THINKING" => {
                                    self.gate.state_changed(GateState::Thinking);
                                    self.publish_state(ClientState::Thinking, "Memahami ucapan...", self.current_expression.clone()).await;
                                }
                                "SPEAKING" => {
                                    self.gate.state_changed(GateState::Speaking);
                                    self.publish_state(ClientState::Speaking, "Menjawab...", self.current_expression.clone()).await;
                                }
                                "ERROR" => {
                                    self.schedule_auto_restart("Server gagal memproses ucapan").await;
                                }
                                _ => {}
                            }
                        }
                    }
                    "transcription" => {
                        let role = val.get("role").and_then(|v| v.as_str()).unwrap_or("assistant").to_string();
                        let text = val.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let is_final = val.get("is_final").and_then(|v| v.as_bool()).unwrap_or(true);
                        if role == "assistant" && is_final && !text.is_empty() {
                            self.last_reply = text.clone();
                        }
                        let _ = self.evt_tx.send(EngineEvent::Transcription { role, text, is_final });
                        self.emit_snapshot(0.0).await;
                    }
                    "utterance_ack" => {
                        let conn_id = val.get("connection_id").and_then(|v| v.as_str()).unwrap_or("");
                        let req_id = val.get("request_id").and_then(|v| v.as_str()).unwrap_or("");
                        let utt_id = val.get("utterance_id").and_then(|v| v.as_str()).unwrap_or("");
                        let stage = val.get("stage").and_then(|v| v.as_str()).unwrap_or("");
                        let next_sample = val
                            .get("next_sample")
                            .or_else(|| val.get("next"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        let total_samples = val
                            .get("total_samples")
                            .or_else(|| val.get("samples"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        let sha = val.get("sha256").and_then(|v| v.as_str());
                        self.upload.ack(
                            conn_id,
                            req_id,
                            utt_id,
                            stage,
                            next_sample,
                            total_samples,
                            sha,
                            chrono_now_millis(),
                        );
                        self.flush_upload_transport().await;
                        if stage == "accepted" {
                            self.processing_deadline = Some(Instant::now() + Duration::from_secs(125));
                        }
                    }
                    "utterance_result" => {
                        let utt_id = val.get("utterance_id").and_then(|v| v.as_str()).unwrap_or("");
                        let status = val.get("status").and_then(|v| v.as_str()).unwrap_or("failed");
                        let code = val.get("code").and_then(|v| v.as_str()).unwrap_or("processing_failed");
                        self.upload.result(utt_id);
                        self.pending_utterance_id = None;
                        self.processing_deadline = None;

                        if self.sheet_owner.is_some() {
                            if status == "completed" {
                                self.enrollment_samples.push(utt_id.to_string());
                                self.recording.sample_count = self.enrollment_samples.len();
                                self.recording.state = "SAMPLES_READY".into();
                                self.recording.message = if self.recording.sample_count > self.recording.required_samples {
                                    "Semua sampel dan holdout diterima. Tekan Simpan profil.".into()
                                } else {
                                    format!(
                                        "Sampel {}/{} diterima.",
                                        self.recording.sample_count,
                                        self.recording.required_samples + 1
                                    )
                                };
                            } else {
                                self.recording.state = if self.enrollment_samples.is_empty() {
                                    "READY".into()
                                } else {
                                    "SAMPLES_READY".into()
                                };
                                self.recording.message = format!("Suara kurang jelas ({code}). Ulangi kalimat dengan jelas.");
                            }
                            self.emit_snapshot(0.0).await;
                            return;
                        }

                        if status != "completed" {
                            if matches!(
                                code,
                                "stt_empty" | "no_speech" | "speaker_rejected" | "quality_insufficient"
                                | "stt_failed" | "hermes_failed" | "tts_failed" | "processing_failed"
                            ) {
                                let hint = match code {
                                    "speaker_rejected" => "Suara tidak cocok dengan pemilik terdaftar.",
                                    "quality_insufficient" => "Suara terlalu singkat atau kurang jelas.",
                                    "stt_empty" | "no_speech" => "Suara belum terdengar jelas, silakan bicara lagi.",
                                    _ => "Kendala layanan sementara, silakan bicara lagi.",
                                };
                                let _ = self.evt_tx.send(EngineEvent::Transcription {
                                    role: "system".into(),
                                    text: hint.into(),
                                    is_final: true,
                                });
                            } else {
                                self.schedule_auto_restart(&format!("Ucapan gagal: {code}")).await;
                                return;
                            }
                        }

                        if self.keep_voice_alive && self.resumed_normal && self.capture_session.is_none() {
                            self.gate.playback_drained();
                            self.gate.state_changed(GateState::Listening);
                            if self.automatic_mode && self.vad_available {
                                let _ = self.start_capture(CapturePurpose::Conversation);
                            }
                            self.publish_state(ClientState::Listening, "Siap, silakan berbicara", self.sleep_expression.clone()).await;
                        }
                    }
                    "utterance_error" => {
                        let code = val.get("code").and_then(|v| v.as_str()).unwrap_or("upload_rejected");
                        if let Some(uid) = val.get("utterance_id").and_then(|v| v.as_str()) {
                            self.upload.server_error(uid, code);
                        }
                        self.schedule_auto_restart(&format!("Unggahan ditolak ({code})")).await;
                    }
                    "audio_owner_ack" => self.handle_owner_ack(&val).await,
                    "enrollment_ack" => self.handle_enrollment_ack(&val).await,
                    "voiceprint_dialog" => {
                        self.recording.voiceprint_available = val.get("available").and_then(|v| v.as_bool());
                        self.recording.is_enrolled = val.get("is_enrolled").and_then(|v| v.as_bool());
                        self.recording.voiceprint_message = val.get("message").and_then(|v| v.as_str()).map(str::to_string);
                        self.emit_snapshot(0.0).await;
                    }
                    "ping" => {
                        let ts = val.get("timestamp").and_then(|v| v.as_i64()).unwrap_or_else(chrono_now_millis);
                        if let Some(c) = self.conn.as_mut() {
                            let _ = c.send_json(&json!({ "type": "pong", "timestamp": ts })).await;
                        }
                    }
                    _ => {}
                }
            }
            Incoming::Binary(pcm) => {
                self.processing_deadline = Some(Instant::now() + Duration::from_secs(30));
                if let Some(pb) = &self.playback {
                    pb.write_pcm16(self.playback_epoch, &pcm);
                }
                let amp = compute_pcm16_rms(&pcm);
                let _ = self.evt_tx.send(EngineEvent::Telemetry { amplitude: amp.max(0.15) });
            }
            Incoming::Pong => {}
        }
    }

    async fn handle_internal(&mut self, msg: InternalMsg) {
        match msg {
            InternalMsg::CaptureLevel(amp, capturing) => {
                let _ = self.evt_tx.send(EngineEvent::Telemetry { amplitude: amp });
                if capturing && self.current_state != ClientState::Listening {
                    self.current_state = ClientState::Listening;
                    self.current_message = "Mendengarkan ucapan...".into();
                    self.emit_snapshot(amp).await;
                } else {
                    let mut w = self.snapshot.write().await;
                    w.amplitude = amp;
                    w.can_finish = capturing;
                }
            }
            InternalMsg::CaptureSealed(mut sealed) => {
                self.stop_capture();
                if sealed.purpose == "diagnostic" {
                    if let Some(diag) = self.diagnostic.as_mut() {
                        let ok = diag.retain(&mut sealed, true).unwrap_or(false);
                        self.recording.diagnostic_ready = ok;
                        self.recording.state = if ok { "LOCAL_READY".into() } else { "ERROR".into() };
                        self.recording.message = if ok {
                            "Klip diagnostik tersimpan privat (5 menit). Putar lokal atau hapus.".into()
                        } else {
                            "Gagal menyimpan klip diagnostik.".into()
                        };
                    }
                    self.emit_snapshot(0.0).await;
                    return;
                }
                if self.conn.is_none() || self.session.is_none() {
                    self.schedule_auto_restart("Koneksi terputus saat mengemas audio").await;
                    return;
                }
                let uid = sealed.id.clone();
                self.pending_utterance_id = Some(uid);
                self.processing_deadline = Some(Instant::now() + Duration::from_secs(125));
                let lease = if sealed.purpose == "enrollment" {
                    self.lease_id.clone()
                } else {
                    None
                };
                let enroll_id = if sealed.purpose == "enrollment" {
                    self.enrollment_id.clone()
                } else {
                    None
                };
                if let Err(e) = self.upload.begin(
                    sealed,
                    lease.as_deref(),
                    enroll_id.as_deref(),
                    chrono_now_millis(),
                ) {
                    self.schedule_auto_restart(&format!("Gagal memulai unggahan: {e}")).await;
                    return;
                }
                self.flush_upload_transport().await;
                if self.sheet_owner.is_some() {
                    self.recording.state = "UPLOADING".into();
                    self.recording.message = "Mengirim sampel ke server...".into();
                    self.emit_snapshot(0.0).await;
                } else {
                    self.publish_state(ClientState::Thinking, "Mengirim audio ke server...", self.sleep_expression.clone()).await;
                }
            }
            InternalMsg::CaptureError(code) => {
                self.schedule_auto_restart(&format!("Mikrofon berhenti: {code}")).await;
            }
            InternalMsg::PlaybackDrained(epoch) => {
                if epoch != self.playback_epoch {
                    return;
                }
                self.playback_drained_at = Some(Instant::now());
                self.gate.playback_drained();
                if self.keep_voice_alive && self.resumed_normal && self.sheet_owner.is_none() {
                    self.gate.state_changed(GateState::Listening);
                    self.publish_state(ClientState::Listening, "Siap, silakan berbicara", self.sleep_expression.clone()).await;
                } else {
                    self.gate.state_changed(GateState::Idle);
                    self.publish_state(ClientState::Idle, "Dijeda", self.sleep_expression.clone()).await;
                }
            }
        }
    }

    async fn on_tick(&mut self) {
        if self.upload.state() != UploadState::Paused {
            self.upload.pump(&mut self.transport, chrono_now_millis());
            self.flush_upload_transport().await;
        }

        if let Some(dl) = self.processing_deadline {
            if Instant::now() >= dl {
                self.schedule_auto_restart("Batas waktu pemrosesan terlampaui").await;
                return;
            }
        }

        if let Some(restart_at) = self.auto_restart_at {
            if Instant::now() >= restart_at && self.keep_voice_alive && self.sheet_owner.is_none() {
                self.auto_restart_at = None;
                self.ensure_connected_and_resume().await;
                return;
            }
        }

        if let Some(renew_at) = self.lease_renew_at {
            if Instant::now() >= renew_at && self.sheet_owner.is_some() && self.lease_id.is_some() {
                self.lease_renew_at = Some(Instant::now() + Duration::from_secs(7));
                let req = Uuid::new_v4().to_string();
                let owner = self.sheet_owner.clone().unwrap_or_default();
                let lease = self.lease_id.clone().unwrap_or_default();
                self.send_v2_control(
                    "audio_owner_renew",
                    &req,
                    json!({ "owner_id": owner, "lease_id": lease }),
                )
                .await;
            }
        }

        // Re-arm automatic VAD capture once post-TTS 600 ms echo guard expires
        if self.keep_voice_alive
            && self.resumed_normal
            && self.automatic_mode
            && self.vad_available
            && self.sheet_owner.is_none()
            && self.capture_session.is_none()
            && self.pending_utterance_id.is_none()
            && self.gate.can_send_audio()
        {
            let guard_elapsed = self
                .playback_drained_at
                .map(|t| t.elapsed() >= Duration::from_millis(600))
                .unwrap_or(true);
            if guard_elapsed {
                let _ = self.start_capture(CapturePurpose::Conversation);
            }
        }
    }

    async fn open_recording_sheet(&mut self, diagnostic_only: bool) {
        self.stop_capture();
        self.resumed_normal = false;
        self.gate.disconnected();
        let owner = Uuid::new_v4().to_string();
        self.sheet_owner = Some(owner);
        self.lease_id = None;
        self.enrollment_id = None;
        self.enrollment_samples.clear();
        self.recording = RecordingSessionSnapshot {
            state: "ACQUIRING".into(),
            message: "Meminta izin kepemilikan mikrofon khusus...".into(),
            ready: false,
            sample_count: 0,
            required_samples: 0,
            diagnostic: diagnostic_only,
            diagnostic_ready: false,
            ..self.recording.clone()
        };
        self.emit_snapshot(0.0).await;

        if self.conn.is_none() {
            if let Some(tok) = self.token.as_deref().filter(|t| !t.trim().is_empty()).map(str::to_string) {
                if let Ok(mut c) = WsConnection::connect_with_auth(&self.server_url, &tok, &self.device_id).await {
                    let hello = ClientHello::new(&self.device_id);
                    if let Ok(v) = serde_json::to_value(&hello) {
                        if c.send_json(&v).await.is_ok() {
                            self.conn = Some(c);
                            return;
                        }
                    }
                }
            }
            self.recording.state = "ERROR".into();
            self.recording.message = "Tidak dapat terhubung ke server. Periksa token/URL.".into();
            self.emit_snapshot(0.0).await;
        } else if self.session.is_some() {
            self.send_acquire().await;
        }
    }

    async fn send_acquire(&mut self) {
        let Some(owner) = self.sheet_owner.clone() else {
            return;
        };
        let req = Uuid::new_v4().to_string();
        self.acquire_request_id = Some(req.clone());
        let purpose = if self.recording.diagnostic {
            "diagnostic"
        } else {
            "enrollment"
        };
        self.send_v2_control(
            "audio_owner_acquire",
            &req,
            json!({ "owner_id": owner, "purpose": purpose, "policy": "cancel_active" }),
        )
        .await;
    }

    async fn handle_owner_ack(&mut self, val: &Value) {
        let state = val.get("state").and_then(|v| v.as_str()).unwrap_or("");
        if let Some(epoch) = val.get("output_epoch").and_then(|v| v.as_u64()) {
            if let Some(s) = self.session.as_mut() {
                s.output_epoch = epoch;
            }
        }
        match state {
            "granted" => {
                self.lease_id = val.get("lease_id").and_then(|v| v.as_str()).map(str::to_string);
                self.lease_renew_at = Some(Instant::now() + Duration::from_secs(7));
                self.recording.ready = true;
                self.recording.state = "READY".into();
                self.recording.message = if self.recording.diagnostic {
                    "Siap merekam klip uji privat (tersimpan lokal maks 5 menit).".into()
                } else {
                    "Siap memuat profil sidik suara.".into()
                };
                if !self.recording.diagnostic {
                    let req = Uuid::new_v4().to_string();
                    self.send_v2_control("voiceprint_query", &req, json!({})).await;
                }
            }
            "released" => {
                self.sheet_owner = None;
                self.lease_id = None;
                self.lease_renew_at = None;
                self.recording = RecordingSessionSnapshot::default();
                if self.resume_after_release && self.keep_voice_alive {
                    self.resume_after_release = false;
                    self.request_resume().await;
                }
            }
            "denied" | "expired" => {
                self.sheet_owner = None;
                self.lease_id = None;
                self.lease_renew_at = None;
                self.recording.ready = false;
                self.recording.state = "ERROR".into();
                self.recording.message = "Kepemilikan mikrofon ditolak atau kedaluwarsa.".into();
            }
            _ => {}
        }
        self.emit_snapshot(0.0).await;
    }

    async fn close_recording_sheet(&mut self, resume: bool) {
        self.stop_capture();
        if let Some(diag) = self.diagnostic.as_mut() {
            let _ = diag.delete();
        }
        self.resume_after_release = resume;
        if let (Some(owner), Some(lease)) = (self.sheet_owner.clone(), self.lease_id.clone()) {
            let req = Uuid::new_v4().to_string();
            self.release_request_id = Some(req.clone());
            self.send_v2_control(
                "audio_owner_release",
                &req,
                json!({ "owner_id": owner, "lease_id": lease, "reason": if resume { "explicit_resume" } else { "dismiss" } }),
            )
            .await;
        }
        self.sheet_owner = None;
        self.lease_id = None;
        self.lease_renew_at = None;
        self.recording = RecordingSessionSnapshot::default();
        if resume && self.keep_voice_alive && self.session.is_some() {
            self.request_resume().await;
        }
        self.emit_snapshot(0.0).await;
    }

    async fn start_sheet_capture(&mut self) {
        if !self.recording.ready {
            return;
        }
        if self.recording.diagnostic {
            if let Err(e) = self.start_capture(CapturePurpose::Diagnostic) {
                self.recording.state = "ERROR".into();
                self.recording.message = format!("Gagal membuka mikrofon: {e}");
            } else {
                self.recording.state = "RECORDING".into();
                self.recording.message = "Merekam klip diagnostik... Tekan Selesai saat cukup.".into();
            }
            self.emit_snapshot(0.0).await;
            return;
        }

        if self.enrollment_id.is_none() {
            let eid = Uuid::new_v4().to_string();
            let req = Uuid::new_v4().to_string();
            self.enrollment_id = Some(eid.clone());
            self.enrollment_request_id = Some(req.clone());
            self.enrollment_operation = Some("begin".into());
            self.recording.state = "STARTING".into();
            self.recording.message = "Meminta sesi pendaftaran dari server...".into();
            let owner = self.sheet_owner.clone().unwrap_or_default();
            let lease = self.lease_id.clone().unwrap_or_default();
            self.send_v2_control(
                "enrollment_begin",
                &req,
                json!({ "owner_id": owner, "lease_id": lease, "enrollment_id": eid }),
            )
            .await;
        } else if let Err(e) = self.start_capture(CapturePurpose::Enrollment) {
            self.recording.state = "ERROR".into();
            self.recording.message = format!("Gagal membuka mikrofon: {e}");
        } else {
            let idx = self.enrollment_samples.len();
            self.recording.state = "RECORDING".into();
            self.recording.message = if idx == self.recording.required_samples {
                "Merekam holdout... Ucapkan satu kalimat jelas lalu tekan Selesai.".into()
            } else {
                format!("Merekam sampel {}... Ucapkan kalimat lalu tekan Selesai.", idx + 1)
            };
        }
        self.emit_snapshot(0.0).await;
    }

    async fn save_enrollment(&mut self) {
        if self.enrollment_samples.len() <= self.recording.required_samples || self.recording.required_samples < 2 {
            return;
        }
        let req = Uuid::new_v4().to_string();
        self.enrollment_request_id = Some(req.clone());
        self.enrollment_operation = Some("save".into());
        let owner = self.sheet_owner.clone().unwrap_or_default();
        let lease = self.lease_id.clone().unwrap_or_default();
        let eid = self.enrollment_id.clone().unwrap_or_default();
        let samples = &self.enrollment_samples[..self.recording.required_samples];
        let holdout = &self.enrollment_samples[self.recording.required_samples];
        self.recording.state = "SAVING".into();
        self.recording.message = "Memvalidasi dan menyimpan profil suara di server...".into();
        self.send_v2_control(
            "enrollment_save",
            &req,
            json!({
                "owner_id": owner,
                "lease_id": lease,
                "enrollment_id": eid,
                "sample_utterance_ids": samples,
                "holdout_utterance_id": holdout
            }),
        )
        .await;
        self.emit_snapshot(0.0).await;
    }

    async fn handle_enrollment_ack(&mut self, val: &Value) {
        let st = val.get("state").and_then(|v| v.as_str()).unwrap_or("");
        match st {
            "collecting" => {
                let req_s = val.get("required_samples").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
                self.recording.required_samples = req_s;
                self.recording.sample_count = 0;
                self.recording.state = "SAMPLES_READY".into();
                self.recording.message = format!("Siap merekam {req_s} sampel dan 1 holdout.");
            }
            "saved" => {
                self.enrollment_id = None;
                self.enrollment_samples.clear();
                self.recording.state = "SAVED".into();
                self.recording.message = "Profil sidik suara berhasil disimpan di server.".into();
                let req = Uuid::new_v4().to_string();
                self.send_v2_control("voiceprint_query", &req, json!({})).await;
            }
            "rejected" | "cancelled" => {
                let code = val.get("code").and_then(|v| v.as_str()).unwrap_or("quality_insufficient");
                self.recording.state = "READY".into();
                self.recording.message = format!("Pendaftaran ditolak ({code}).");
            }
            _ => {}
        }
        self.emit_snapshot(0.0).await;
    }

    async fn play_diagnostic(&mut self) {
        let Some(diag) = self.diagnostic.as_mut() else {
            return;
        };
        let Ok(Some(pcm)) = diag.read_pcm_if_valid() else {
            self.recording.diagnostic_ready = false;
            self.recording.state = "READY".into();
            self.recording.message = "Klip kedaluwarsa atau belum direkam.".into();
            self.emit_snapshot(0.0).await;
            return;
        };
        if let Some(pb) = self.playback.clone() {
            let epoch = pb.reset_epoch();
            self.playback_epoch = epoch;
            tokio::task::spawn_blocking(move || {
                pb.write_pcm16_with_rate(epoch, &pcm, 16_000);
                pb.wait_drained(epoch, Duration::from_secs(21));
            });
            self.recording.message = "Memutar klip diagnostik lokal...".into();
        } else {
            self.recording.message = "Perangkat output audio tidak tersedia.".into();
        }
        self.emit_snapshot(0.0).await;
    }

    fn start_capture(&mut self, purpose: CapturePurpose) -> anyhow::Result<()> {
        self.stop_capture();
        let use_auto = purpose == CapturePurpose::Conversation && self.automatic_mode && self.vad_available;
        let vad_opt = if use_auto {
            self.vad_model.as_ref().and_then(|p| SileroVad::new(p).ok())
        } else {
            None
        };
        let is_auto = vad_opt.is_some();
        let mut buf = UtteranceBuffer::default();
        if !is_auto {
            buf.start(purpose);
        }
        let shared = Arc::new(SharedCaptureControl {
            buffer: Mutex::new(buf),
            purpose: Mutex::new(purpose),
            finish_now: std::sync::atomic::AtomicBool::new(false),
            manual_start_now: std::sync::atomic::AtomicBool::new(false),
        });
        let shared_cb = Arc::clone(&shared);
        let tx = self.internal_tx.clone();
        let mut vad = vad_opt;
        let mut seg = SpeechSegmenter::new();
        let mut vad_fifo: Vec<i16> = Vec::with_capacity(1024);
        let mut vad_win = [0.0f32; 512];
        let mut last_level_emit = Instant::now();

        let cap = audio::start_capture(&CaptureConfig::default(), move |frame| {
            let now_ms = chrono_now_millis();
            if shared_cb.finish_now.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let mut b = shared_cb.buffer.lock().unwrap();
                if let Some(sealed) = b.finish("manual_finish", now_ms) {
                    let _ = tx.send(InternalMsg::CaptureSealed(sealed));
                }
                return;
            }
            if shared_cb.manual_start_now.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let mut b = shared_cb.buffer.lock().unwrap();
                let p = *shared_cb.purpose.lock().unwrap();
                b.start(p);
            }
            {
                let mut b = shared_cb.buffer.lock().unwrap();
                if let Some(sealed) = b.accept(&frame, now_ms) {
                    let _ = tx.send(InternalMsg::CaptureSealed(sealed));
                    return;
                }
            }
            if let Some(v) = vad.as_mut() {
                vad_fifo.extend_from_slice(&frame);
                while vad_fifo.len() >= 512 {
                    let chunk: Vec<i16> = vad_fifo.drain(..512).collect();
                    prepare_vad_window(&chunk, &mut vad_win);
                    if let Ok(prob) = v.probability(&vad_win) {
                        if let Ok(boundary) = seg.boundary(prob) {
                            let mut b = shared_cb.buffer.lock().unwrap();
                            match boundary {
                                Boundary::Start => {
                                    let p = *shared_cb.purpose.lock().unwrap();
                                    b.start(p);
                                }
                                Boundary::End => {
                                    if let Some(sealed) = b.finish("vad_end", now_ms) {
                                        let _ = tx.send(InternalMsg::CaptureSealed(sealed));
                                        return;
                                    }
                                }
                                Boundary::None => {}
                            }
                        }
                    }
                }
            }
            if last_level_emit.elapsed() >= Duration::from_millis(45) {
                last_level_emit = Instant::now();
                let b = shared_cb.buffer.lock().unwrap();
                let _ = tx.send(InternalMsg::CaptureLevel(b.rms(), b.is_capturing()));
            }
        })?;

        self.capture_session = Some(cap);
        self.capture_shared = Some(shared);
        Ok(())
    }

    fn stop_capture(&mut self) {
        if let Some(cap) = self.capture_session.take() {
            cap.stop();
        }
        self.capture_shared = None;
    }

    fn matches_output_epoch(&self, val: &Value) -> bool {
        let Some(sess) = &self.session else {
            return false;
        };
        let conn_ok = val.get("connection_id").and_then(|v| v.as_str()) == Some(sess.connection_id.as_str());
        let epoch_ok = val.get("output_epoch").and_then(|v| v.as_u64()) == Some(sess.output_epoch);
        conn_ok && epoch_ok
    }

    async fn send_v2_control(&mut self, msg_type: &str, request_id: &str, extra: Value) {
        let Some(sess) = &self.session else {
            return;
        };
        let mut obj = match extra {
            Value::Object(m) => m,
            _ => serde_json::Map::new(),
        };
        obj.insert("type".into(), json!(msg_type));
        obj.insert("connection_id".into(), json!(sess.connection_id));
        obj.insert("request_id".into(), json!(request_id));
        if let Some(c) = self.conn.as_mut() {
            if let Err(e) = c.send_json(&Value::Object(obj)).await {
                self.schedule_auto_restart(&format!("Gagal mengirim kontrol: {e}")).await;
            }
        }
    }

    async fn flush_upload_transport(&mut self) {
        let jsons: Vec<Value> = self.transport.queued_out_json.drain(..).collect();
        let bins: Vec<Vec<u8>> = self.transport.queued_out_bin.drain(..).collect();
        if let Some(c) = self.conn.as_mut() {
            for v in jsons {
                if let Err(e) = c.send_json(&v).await {
                    self.schedule_auto_restart(&format!("Gagal mengirim kontrol unggahan: {e}")).await;
                    return;
                }
            }
            for b in bins {
                if let Err(e) = c.send_binary(&b).await {
                    self.schedule_auto_restart(&format!("Gagal mengirim frame audio: {e}")).await;
                    return;
                }
            }
        }
    }

    async fn publish_state(&mut self, state: ClientState, message: &str, expr: Option<String>) {
        self.current_state = state;
        self.current_message = message.to_string();
        if expr.is_some() {
            self.current_expression = expr;
        }
        self.emit_snapshot(0.0).await;
    }

    async fn emit_snapshot(&self, amplitude: f32) {
        let can_finish = self
            .capture_shared
            .as_ref()
            .map(|sh| sh.buffer.lock().unwrap().is_capturing())
            .unwrap_or(false);
        let snap = EngineSnapshot {
            state: self.current_state,
            message: self.current_message.clone(),
            voice_enabled: self.keep_voice_alive,
            automatic_mode: self.automatic_mode,
            vad_available: self.vad_available,
            expression: self.current_expression.clone(),
            amplitude,
            can_finish,
            last_reply: self.last_reply.clone(),
            recording: self.recording.clone(),
            server_url: self.server_url.clone(),
            device_id: self.device_id.clone(),
            has_token: self
                .token
                .as_deref()
                .map(|t| !t.is_empty() && t != crate::config::TOKEN_PLACEHOLDER)
                .unwrap_or(false),
        };
        *self.snapshot.write().await = snap.clone();
        let _ = self.evt_tx.send(EngineEvent::Snapshot(snap));
    }
}

fn compute_pcm16_rms(pcm_bytes: &[u8]) -> f32 {
    if pcm_bytes.len() < 2 {
        return 0.0;
    }
    let mut sum = 0.0f64;
    let mut count = 0usize;
    let mut i = 0;
    while i + 1 < pcm_bytes.len() {
        let s = i16::from_le_bytes([pcm_bytes[i], pcm_bytes[i + 1]]) as f64;
        sum += s * s;
        count += 1;
        i += 2;
    }
    if count == 0 {
        0.0
    } else {
        ((sum / count as f64).sqrt() / 32768.0).clamp(0.0, 1.0) as f32
    }
}
