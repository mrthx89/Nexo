# Windows Nexo — Phase 2 UI handoff notes

- Branch (repo Nexo): `feat/windows-nexo-phase2-ui` (source dipersiapkan di server Linux ini; **build/dev/bundling MSI dilakukan di lokal Windows user**).
- Lokasi source UI: `clients/nexo-windows/nexo-ui/` (Tauri v2 + SvelteKit + Tailwind).
- Backend boundary: TS `src/lib/backend/*` dan Rust `src-tauri/src/backend.rs` (trait `NexoBackendEngine` + `MockBackend`) berbagi snapshot/state machine yang sama; siap ditempel ke `nexo-core` (Phase 1) tanpa ubah kontrak UI.
- Komponen: `StatusCapsule` (notch/Dynamic Island), `MascotStage` (16 ekspresi dari aset Android + glow/saccade/pixel-shift), `ConversationPanel` (transkrip + salin), `MicButton` (hijau/aktif + waveform 4 bar), `SettingsSheet` (token/URL).
- Jendela: `main` (kiosk `/`) + `capsule` (`/capsule`, always-on-top, dekorasi off, transparan, bisa hide/show; klik toggle main). Tray minimal (status, toggle mic, settings, show/hide, quit; close main → hide to tray).
- Validasi di host Linux ini: `npm install` + `npm run build` (frontend/static) sukses, `npm run check` (svelte-check) 0 error/0 warning. `cargo check` Tauri Rust dan `npm run tauri build` (MSI) **harus dijalankan di Windows lokal** (butuh MSVC/WebView2/WiX/toolchain Windows).
- Mock: simulasi state/telemetry/transkrip mengalir lewat interface backend yang sama dengan integrasi nyata (bukan hardcode komponen).
