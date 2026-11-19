//! `nexo-core` — the headless Rust core for the Nexo appliance.
//!
//! This crate ports the Nexo Android client's audio capture, Silero VAD,
//! WebSocket transport, protocol data model, outdoor negotiation, half-duplex
//! gate, and the v2 utterance upload state machine into a headless library.
//! The Windows binary (`nexo`) drives these modules; there is no UI here.
//!
//! Module map (Android → Rust):
//!   * `network/ProtocolModels.kt`  → [`protocol`]
//!   * `network/OutdoorProtocol.kt` → [`protocol::negotiate`]
//!   * `audio/HalfDuplexGate.kt`    → [`half_duplex`]
//!   * `audio/vad/SileroVad.java`   → [`vad`]
//!   * `audio/core/LocalCapture.java` + `CapturePump.java` → [`audio`]
//!   * `audio/core/UploadSession.java` → [`upload`]
//!   * `network/NexoWebSocketClient.kt` → [`ws`]

pub mod audio;
pub mod config;
pub mod half_duplex;
pub mod protocol;
pub mod upload;
pub mod vad;
pub mod ws;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");