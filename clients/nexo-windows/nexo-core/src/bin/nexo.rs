//! `nexo` — the headless Nexo appliance client for Windows.
//!
//! Drives the modules in `nexo-core`: connect → capture PCM 16 kHz mono →
//! Silero VAD → upload utterance chunks → receive transcription/state_change →
//! play response → half-duplex gate. There is no UI here; this is the engine
//! that a future Tauri/tray shell will wrap.
//!
//! The `smoke` subcommand exercises the full handshake + upload-chunk flow
//! against an in-memory fixture transport (no live server required), so CI can
//! verify the wire logic without `wss://nexo.yhdev.my.id`.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use nexo_core::config::{Config, TOKEN_PLACEHOLDER};
use nexo_core::half_duplex::HalfDuplexGate;
use nexo_core::protocol::{self, ClientHello};
use nexo_core::upload::{self, Sealed, UploadSession, Transport};

#[derive(Parser)]
#[command(name = "nexo", version, about = "Nexo headless appliance client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Connect to the server and run the capture → VAD → upload loop.
    Run {
        /// Override the server URL (else env / config file / default).
        #[arg(long)]
        server: Option<String>,
        /// Override the device id.
        #[arg(long)]
        device_id: Option<String>,
    },
    /// Exercise handshake + upload-chunk against a fixture (no live server).
    Smoke,
    /// Print the resolved configuration and exit.
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::try_init().ok();
    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Run {
        server: None,
        device_id: None,
    }) {
        Command::Config => {
            let cfg = Config::load()?;
            println!("server_url = {}", cfg.server_url);
            println!("device_id  = {}", cfg.device_id);
            println!(
                "token      = {}",
                if cfg.has_real_token() {
                    "<redacted>"
                } else {
                    TOKEN_PLACEHOLDER
                }
            );
            if let Some(v) = &cfg.vad_model {
                println!("vad_model  = {}", v.display());
            }
            Ok(())
        }
        Command::Smoke => smoke(),
        Command::Run { server, device_id } => run(server, device_id).await,
    }
}

/// The fixture smoke test: simulate a handshake and an upload-chunk exchange
/// against an in-memory transport. This is the deliverable proof that the
/// protocol + upload logic are correct without a live server.
fn smoke() -> Result<()> {
    println!("== nexo smoke: fixture handshake + upload chunk ==");

    // 1. Build client_hello exactly as the Android client does.
    let hello = ClientHello::new("smoke-device");
    let hello_json = serde_json::to_string(&hello)?;
    println!("-> client_hello: {hello_json}");

    // 2. Fixture server replies with a valid server_hello (outdoor v2).
    let server_hello = serde_json::json!({
        "type": "server_hello",
        "session_id": "sess-smoke",
        "agent_name": "Nexo",
        "status": "ready",
        "protocol_version": "2.0",
        "conversation_paused": true,
        "connection_id": "00000000-0000-0000-0000-000000000001",
        "server_instance_id": "00000000-0000-0000-0000-000000000002",
        "output_epoch": 1,
        "capabilities": ["utterance_upload_v2", "exclusive_audio_v1"],
        "limits": {
            "max_samples": 320000,
            "retry_ttl_ms": 30000,
            "owner_lease_ms": 15000,
            "max_chunk_bytes": 1024
        }
    });
    let session = protocol::negotiate(&server_hello)
        .context("negotiate server_hello")?;
    println!("<- server_hello: negotiated (max_chunk_bytes={})", session.max_chunk_bytes);

    // 3. Half-duplex gate opens for the turn.
    let gate = HalfDuplexGate::new();
    gate.connected();
    gate.microphone_ready(true);
    assert!(gate.can_send_audio());
    println!("half-duplex: gate open (can_send_audio=true)");

    // 4. Sealed utterance + upload session against a mock transport.
    let mut up = UploadSession::new();
    up.connect(&session, true, 0)?;
    let pcm = vec![0i16; 4_000]; // 8000 bytes = 8 chunks of 1024
    let sealed = Sealed::new(
        pcm.iter().flat_map(|s| s.to_le_bytes()).collect(),
        "conversation",
        "vad_end",
        0,
    );
    let expected_hash = sealed.sha256().to_string();

    let mut wire = FixtureTransport::new();
    up.begin(sealed, None, None, 0)?;
    // UploadSession builds control frames in last_control; flush them to wire.
    if let Some(ctrl) = up.take_last_control() {
        wire.control(&ctrl);
    }

    // 5. Drive the state machine against the fixture: begin ack → pump chunks
    //    → receiving acks → commit → accepted.
    drive_upload(&mut up, &mut wire, &session, &expected_hash)?;

    println!("upload: state={:?}, chunks={}", up.state(), wire.binary_frames.len());
    println!("== smoke OK ==");
    Ok(())
}

/// Drive the upload state machine to completion using the fixture transport.
fn drive_upload(
    up: &mut UploadSession,
    wire: &mut FixtureTransport,
    session: &nexo_core::protocol::OutdoorSession,
    expected_hash: &str,
) -> Result<()> {
    // Begin ack: read the begin request id from the control frame we emitted.
    let begin_ctrl = wire
        .control_frames
        .last()
        .context("begin control frame")?
        .clone();
    let begin_req = begin_ctrl["request_id"].as_str().unwrap_or_default().to_string();
    let utter_id = begin_ctrl["utterance_id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_default();
    up.ack(&session.connection_id, &begin_req, &utter_id, "begin", 0, 0, None, 1);

    let mut now = 2i64;
    let mut guard = 0;
    while up.state() != upload::UploadState::Commit && guard < 100 {
        guard += 1;
        up.pump(wire, now);
        // Flush any control frame produced by pump (e.g. utterance_commit).
        if let Some(ctrl) = up.take_last_control() {
            wire.control(&ctrl);
        }
        now += 1;
        if up.state() == upload::UploadState::Commit {
            break;
        }
        // Server confirms receipt of all sent samples (receiving acks do not
        // require a request_id that matches an outgoing control frame).
        up.ack(
            &session.connection_id,
            "",
            &utter_id,
            "receiving",
            up_sanity_sent(wire),
            4_000,
            None,
            now,
        );
    }
    assert_eq!(up.state(), upload::UploadState::Commit, "did not reach Commit (state={:?})", up.state());

    // Commit ack → accepted, with the integrity hash.
    let commit_ctrl = wire
        .control_frames
        .iter()
        .rev()
        .find(|c| c["type"].as_str() == Some("utterance_commit"))
        .cloned()
        .context("commit control frame")?;
    let commit_req = commit_ctrl["request_id"].as_str().unwrap_or_default().to_string();
    up.ack(
        &session.connection_id,
        &commit_req,
        &utter_id,
        "accepted",
        0,
        4_000,
        Some(expected_hash),
        now,
    );
    assert_eq!(up.state(), upload::UploadState::Accepted, "not accepted (state={:?}, err={:?})", up.state(), up.error());
    Ok(())
}

/// The fixture never had a real sent-sample count exposed; recover it from the
/// binary frames we captured (each frame is 28-byte header + payload).
fn up_sanity_sent(wire: &FixtureTransport) -> u32 {
    wire.binary_frames
        .iter()
        .map(|f| (f.len().saturating_sub(28) / 2) as u32)
        .sum()
}

/// Connect to the server and run the full `NexoEngine` live audio loop.
async fn run(server: Option<String>, device_id: Option<String>) -> Result<()> {
    let cfg = Config::load()?;
    let cred_store = nexo_core::credential_store::DeviceCredentialStore::default_store();
    let server_url = server
        .or_else(|| cred_store.saved_server_url())
        .unwrap_or(cfg.server_url.clone());
    let device_id = device_id
        .or_else(|| cred_store.load_or_create_device_id().ok())
        .unwrap_or(cfg.device_id.clone());

    let token = if cfg.has_real_token() {
        Some(cfg.bearer_token.clone())
    } else {
        cred_store.read_token(&server_url, &device_id)
    };
    if token.is_none() {
        tracing::warn!(
            "bearer token is missing; set {ENV_HINT} or save credentials via Nexo Desktop"
        );
    }

    let handle = nexo_core::engine::spawn_engine(nexo_core::engine::EngineConfig {
        server_url: server_url.clone(),
        device_id: device_id.clone(),
        token,
        vad_model: cfg.vad_model.clone(),
    });
    let mut rx = handle.subscribe();
    handle.send(nexo_core::engine::EngineCommand::StartVoice)?;
    println!("nexo headless running on {server_url} (device {device_id}); press Ctrl+C to stop");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                let _ = handle.send(nexo_core::engine::EngineCommand::Shutdown);
                break;
            }
            ev = rx.recv() => {
                match ev {
                    Ok(nexo_core::engine::EngineEvent::Snapshot(s)) => {
                        println!("[{:?}] {}", s.state, s.message);
                    }
                    Ok(nexo_core::engine::EngineEvent::Transcription { role, text, is_final }) => {
                        if is_final {
                            println!("[{role}] {text}");
                        }
                    }
                    Ok(nexo_core::engine::EngineEvent::Telemetry { .. }) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        }
    }
    Ok(())
}

const ENV_HINT: &str = "NEXO_BEARER_TOKEN";

/// A fixture transport that records every control and binary frame, used by
/// the smoke test.
struct FixtureTransport {
    control_frames: Vec<serde_json::Value>,
    binary_frames: Vec<Vec<u8>>,
}

impl FixtureTransport {
    fn new() -> Self {
        Self {
            control_frames: Vec::new(),
            binary_frames: Vec::new(),
        }
    }
}

impl Transport for FixtureTransport {
    fn control(&mut self, value: &serde_json::Value) -> bool {
        self.control_frames.push(value.clone());
        true
    }
    fn binary(&mut self, value: &[u8]) -> bool {
        self.binary_frames.push(value.to_vec());
        true
    }
    fn queued_bytes(&self) -> u64 {
        0
    }
}