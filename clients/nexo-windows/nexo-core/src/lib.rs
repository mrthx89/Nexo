//! `nexo-core` — Core Rust library for the Nexo Windows appliance client.

pub mod audio;
pub mod buffer;
pub mod config;
pub mod credential_store;
pub mod diagnostic;
pub mod engine;
pub mod half_duplex;
pub mod iot_http;
pub mod iot_wiz;
pub mod protocol;
pub mod segmenter;
pub mod upload;
pub mod vad;
pub mod ws;

/// Crate version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
