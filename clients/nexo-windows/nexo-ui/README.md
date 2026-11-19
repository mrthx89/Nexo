# Nexo Windows UI (Phase 2) — Tauri v2 + Svelte + Tailwind

UI kiosk Nexo untuk Windows (Phase 2): jendela kiosk utama + **capsule/notch always-on-top** ala Dynamic Island, terhubung ke state machine melalui **backend boundary interface** yang sama untuk mock dan integrasi nyata `nexo-core` (Phase 1).

> **Build dilakukan di lokal Windows user (bukan server Linux ini).** Repo/server ini menyiapkan source yang siap di-pull. MSI unsigned (dev) dihasilkan di mesin Windows.

## Arsitektur

- **Frontend**: SvelteKit + Tailwind (`src/`), static adapter (`build/`).
- **Shell**: Tauri v2 (`src-tauri/`), dua jendela:
  - `main` (`/`) — kiosk: `StatusCapsule`, `MascotStage` (16 ekspresi + eye glow/saccade/pixel-shift), `ConversationPanel` (transkrip + salin), `MicButton` (hijau/aktif, waveform 4 bar), `SettingsSheet` (token, URL).
  - `capsule` (`/capsule`) — notch always-on-top, dekorasi off, transparan: status (Menghubungkan/Siaga/Menyimak/Memproses/Menjawab/Paused) + equalizer kecil; bisa di-hide, klik untuk toggle jendela utama.
- **Tray (minimal, Phase 2)**: ikon tray + menu Status, Toggle Mic (Play/Pause), Buka Settings, Tampilkan/Sembunyikan Jendela & Capsule, Keluar. Close main window → hide to tray (Quit di tray keluar bersih).
- **Backend boundary (kontrak sama untuk mock & nyata)**:
  - Frontend: `src/lib/backend/types.ts` (`NexoBackend`, `BackendSnapshot`), `mockBackend.ts`, `index.ts` (`TauriNexoBackend` memakai `invoke`/event Tauri bila tersedia, fallback ke mock).
  - Tauri/Rust: `src-tauri/src/backend.rs` trait `NexoBackendEngine` + `MockBackend` (state machine + `visual_telemetry` simulasi + transkrip) → emit event `nexo://snapshot`, command `backend_*`.
  - Integrasi nyata: ganti/extend engine agar memakai `nexo-core` (audio/VAD/WSS, Phase 1) **tanpa mengubah bentuk snapshot/command** yang dikonsumsi UI.

## State & ekspresi (mengacu Android)

- State: `CONNECTING, IDLE, LISTENING, PROCESSING, ANSWERING, PAUSED` → label ID: Menghubungkan, Siaga, Menyimak, Memproses, Menjawab, Paused (lihat `NexoState.kt`).
- 16 ekspresi maskot dari `MascotExpression.kt`/aset Android (`public/mascots/`): netral, senang, sangat_senang, tertawa, bingung, mengantuk, mengedip, terkejut, cinta, jahil, keren, malu, marah, menangis, sedih, takut, sleeping. `MascotStage` menambahkan aura/eye-glow, bob/saccade halus, dan pixel-shift periodik (burn-in mitigation) berbasis telemetry/phase.
- `visual_telemetry`: `amplitude` + `frequencyBands` (4 bar) menggerakkan equalizer capsule & `MicButton`/stage (lihat `VisualizerView.kt`, `LiveWaveformView.kt`).

## Prasyarat build lokal (Windows)

1. **Node.js** (LTS) + npm.
2. **Rust (stable, MSVC)**: `rustup default stable`, target `x86_64-pc-windows-msvc`.
3. **Visual Studio Build Tools** (MSVC C++ + Windows SDK).
4. **WebView2 Runtime** (biasanya sudah ada di Windows 10/11).
5. (Opsional integrasi nyata) aset/model/config `nexo-core` sesuai `clients/nexo-windows/README.md`.

## Jalankan & build

Dari `clients/nexo-windows/nexo-ui/`:

```powershell
npm install

# Frontend saja (browser) — mock backend, cocok untuk cek UI cepat
npm run dev

# Tauri dev (jendela native + tray + capsule) — mock backend
npm run tauri dev

# Produksi (frontend + bundle)
npm run build
npm run tauri build
```

`npm run tauri build` menghasilkan bundle/installer (MSI via WiX, unsigned dev) di `src-tauri/target/release/bundle/` pada Windows. Build biner/installer Windows **tidak** diklaim dari Linux host; source ini siap dipull dan dibuild lokal.

## Integrasi `nexo-core` (nyata)

- Boundary siap: trait/engine Rust (`src-tauri/src/backend.rs`) dan interface TS (`src/lib/backend/*`) berbagi bentuk snapshot `{ state, expression, telemetry{amplitude,frequencyBands}, transcript, micEnabled, connected, statusText, serverUrl, deviceId, hasToken }`.
- Langkah integrasi: tambahkan dependensi/path `nexo-core` di `src-tauri/Cargo.toml` (feature flag), implementasikan engine yang memetakan event/state nexo-core ke `BackendSnapshot` dan terus emit `nexo://snapshot`; wiring VAD/audio/WSS tetap di `nexo-core` (out of scope Phase 2).
- Token/URL: `SettingsSheet` mengirim ke `backend_update_settings` (tidak di-log). Jangan commit kredensial nyata.

## Catatan

- UI simulasi/mock: state berputar dan telemetry/transkrip contoh mengalir lewat interface yang sama dengan backend nyata (bukan hardcode di komponen).
- Scope Phase 2: tray minimal + capsule window; detail tray selalu-on/autorun lanjutan dikerjakan Phase 3.
