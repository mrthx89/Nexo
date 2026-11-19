# nexo-client (Windows Nexo — Phase 1: Headless Rust core)

Headless Rust core for the Windows Nexo appliance client. This crate mirrors the Android client’s audio/VAD/WebSocket/utterance-upload stack (without UI) so it can be embedded by a future Tauri v2 + Svelte/Tailwind shell.

## Scope (Phase 1)

- `nexo-core` library crate with modules: `protocol`, `half_duplex`, `upload`, `vad`, `audio`, `ws`, `config`
- Headless binary `nexo` with `smoke` (fixture, no live server), `config`, and `run` subcommands
- Wire protocol identical to Android (`client_hello`, v2 outdoor negotiation, utterance upload v2 lease/epoch/chunk state machine)
- Silero VAD v6.2.3 (ONNX via `ort`) with pinned SHA-256 integrity check matching the Android model
- **No server/protocol changes. No real credentials** — token is read from env/config and defaults to `[REDACTED]`.

## Repository layout

```
clients/nexo-windows/
├─ Cargo.toml                 # virtual workspace
├─ Cargo.lock
├─ .gitignore
└─ nexo-core/
   ├─ Cargo.toml
   └─ src/
      ├─ lib.rs
      ├─ protocol.rs          # ProtocolModels + outdoor negotiation
      ├─ half_duplex.rs       # HalfDuplexGate port
      ├─ upload.rs            # v2 utterance upload + chunk/lease/epoch + SHA-256 ack
      ├─ vad.rs               # Silero v6.2.3 (64 ctx + 512 frame = 576, 2x1x128 state)
      ├─ audio.rs             # cpal capture/playback helpers (16 kHz mono PCM16)
      ├─ ws.rs                # tokio-tungstenite + rustls transport
      ├─ config.rs            # URL/device/token/model config (env/file, placeholder token)
      └─ bin/nexo.rs          # headless CLI (smoke / run / config)
```

## Configuration (no secrets)

- `NEXO_SERVER_URL` (default `wss://nexo.yhdev.my.id/ws/appliance`, placeholder)
- `NEXO_DEVICE_ID` (default `nexo-headless`)
- `NEXO_BEARER_TOKEN` — **never hardcode real tokens**. If unset/missing it falls back to the placeholder `[REDACTED]` and `run` warns; live auth requires a real token supplied at runtime.
- `NEXO_VAD_MODEL` — path to `silero-v6.2.3.onnx`
- `NEXO_CONFIG` — optional TOML config file (fields: `server_url`, `device_id`, `token`, `vad_model`). Example: `cargo run -p nexo-core -- config` prints resolved config with token redacted.

Pinned VAD model SHA-256: `1a153a22f4509e292a94e67d6f9b85e8deb25b4988682b7e174c65279d8788e3` (same as Android client), verified on load.

## Build & test on Windows (local deliverable)

> `cargo build --release` producing a **Windows binary** is **not** done on this Linux server. This is source intended to be pulled and built on Windows.

1. Install **Rust (stable, MSVC)**:
   - `rustup default stable`
   - target: `x86_64-pc-windows-msvc`
2. Install **Visual Studio Build Tools** (MSVC C++ toolchain + Windows SDK). For later Tauri phases also install **WebView2 Runtime** and Tauri Windows prerequisites.
3. Ensure the Silero model file is available locally and point `NEXO_VAD_MODEL` to it (SHA-256 must match the pinned value above). You can reuse `clients/nexo-android/app/src/main/assets/vad/silero-v6.2.3.onnx` from this repo.
4. From `clients/nexo-windows/`:
   ```powershell
   cargo test -p nexo-core
   cargo run -p nexo-core -- smoke
   cargo run -p nexo-core -- config
   cargo build --release -p nexo-core
   ```
5. Real run (requires a valid bearer token supplied at runtime, never committed):
   ```powershell
   $env:NEXO_SERVER_URL="wss://nexo.yhdev.my.id/ws/appliance"
   $env:NEXO_DEVICE_ID="nexo-windows"
   $env:NEXO_BEARER_TOKEN="<real token at runtime>"
   $env:NEXO_VAD_MODEL="C:\path\to\silero-v6.2.3.onnx"
   cargo run --release -p nexo-core -- run
   ```

Notes:
- Audio capture/playback uses `cpal` (WASAPI on Windows). Device permissions and exclusive-mode tuning are runtime/environment concerns.
- `ort` is configured with `download-binaries` to obtain a prebuilt ONNX Runtime for the target.
- Do **not** treat Linux-host artifacts as Windows deliverables.

## Linux-host verification (done, logic/CI parity)

Verified on Linux host (Rust 1.100.0, cargo 1.100.0) with pkg-config/sysroot deps available:

- `cargo test -p nexo-core` → **43 passed, 0 failed** (lib/unit tests covering `protocol`, `half_duplex`, `upload`, `vad`, `config`, `ws`, `audio` helpers; 0 bin tests, 0 doctests)
- `cargo run -p nexo-core --bin nexo -- smoke` → **OK** (fixture handshake + upload chunk; `upload: state=Accepted, chunks=8`)

This confirms protocol/half-duplex/upload state-machine logic and the fixture smoke path on Linux. **Runtime validation on Windows (WASAPI mic capture, device I/O, tray/notch integration in later phases, MSI packaging) must still be performed on a local Windows machine.**

## Protocol / integrity notes

- `server_hello` negotiation requires `protocol_version=2.0`, `conversation_paused=true`, capabilities `utterance_upload_v2` + `exclusive_audio_v1`, and strict limits (`max_samples=320000`, `retry_ttl_ms=30000`, `owner_lease_ms=15000`, even `max_chunk_bytes <= 16384`).
- Utterance upload is integrity-gated: local queue admission is not an ACK; buffers are released only on server `accepted/completed` with matching sample count and SHA-256.
- Half-duplex gate mirrors Android semantics: capture is allowed only when ready+mic and not in thinking/speaking/playback-pending.

## Status (Phase 1)

Delivered in this workspace: headless core source, fixture smoke path, and protocol/half-duplex/upload/VAD/audio/ws/config logic suitable for integration by Phase 2 (Tauri v2 UI). Live end-to-end audio against the production server must be validated on Windows with a real token and model present.
