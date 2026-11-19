# nexo-client (Windows Nexo — Rust `nexo-core` + Tauri v2 Desktop)

Lightweight Windows client for Nexo Dedicated Appliance built with a native Rust core (`nexo-core`) and a minimal Tauri v2 WebView2 shell (`nexo-desktop`), reusing the exact v2 outdoor protocol, Silero VAD v6.2.3 model, and NEXO Silver mascot SVGs from Android.

## Scope & Architecture

- `nexo-core/` — Native Rust core (audio capture + resampler to 16 kHz mono PCM16, 24 kHz TTS playback, Silero VAD via `ort`, v2 `NXA2` chunk upload, half-duplex + always-on reconnect engine, DPAPI token store, WiZ LAN UDP port `38899`, IoT/Polytron AC HTTP client, private diagnostic WAV clip).
- `nexo-desktop/` — Lightweight Tauri v2 GUI shell (pure Vanilla HTML/CSS/JS + 17 NEXO Silver SVG expressions, system tray residency, Setup/Voiceprint/IoT modals).
- `clients/nexo-windows/DESIGN.md` — Explicit anti-slop design system tokens and contrast/accessibility specification.

## Repository layout

```
clients/nexo-windows/
├─ Cargo.toml                 # nexo-core workspace
├─ Cargo.lock
├─ DESIGN.md                  # antislop design direction & WCAG AA palette
├─ .gitignore
├─ nexo-core/
│  ├─ Cargo.toml
│  └─ src/
│     ├─ lib.rs
│     ├─ protocol.rs          # ProtocolModels + outdoor v2 negotiation
│     ├─ half_duplex.rs       # HalfDuplexGate
│     ├─ segmenter.rs         # SpeechSegmenter (onset 0.50, hangover 32)
│     ├─ buffer.rs            # UtteranceBuffer + pre-roll ring + VAD gain
│     ├─ upload.rs            # v2 utterance upload + chunk/lease/epoch + SHA-256 ack
│     ├─ vad.rs               # Silero v6.2.3 (64 ctx + 512 frame = 576, 2x1x128 state)
│     ├─ audio.rs             # cpal WASAPI capture resampler & 24 kHz TTS playback
│     ├─ ws.rs                # tokio-tungstenite + rustls auth transport
│     ├─ config.rs            # WSS validation + origin helpers + TOML/env config
│     ├─ credential_store.rs  # Origin-bound DPAPI token protection
│     ├─ diagnostic.rs        # Opt-in private 5-minute diagnostic WAV clip
│     ├─ iot_wiz.rs           # Philips WiZ LAN UDP broadcast/control
│     ├─ iot_http.rs          # Server /iot/* and Polytron AC REST client
│     ├─ engine.rs            # Full async NexoEngine orchestrator + always-on loop
│     └─ bin/nexo.rs          # Headless CLI (smoke / run / config)
└─ nexo-desktop/
   ├─ ui/                     # Vanilla HTML/CSS/JS + 17 NEXO Silver SVGs
   └─ src-tauri/              # Tauri v2 host + system tray + IPC commands
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
5. Build & run GUI Desktop (`clients/nexo-windows/nexo-desktop/src-tauri/`):
   ```powershell
   cargo run --release
   # atau untuk installer Windows (.msi / .exe):
   cargo tauri build
   ```
6. Real CLI run (requires a valid bearer token supplied at runtime, never committed):
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
