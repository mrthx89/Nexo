# Windows Nexo — Phase 3: Tray always-on, minimize-to-tray, autostart

Branch: `feat/windows-nexo-phase3-tray` (berbasis Phase 2 UI `feat/windows-nexo-phase2-ui`).
Source path: `clients/nexo-windows/nexo-ui/` (Tauri v2 + SvelteKit/Tailwind).

> **Build & verifikasi runtime dilakukan di lokal Windows user (bukan server Linux ini).**
> `cargo check`/`tauri dev`/`tauri build` (MSI) membutuhkan toolchain Windows (MSVC), WebView2 Runtime, dan WiX (untuk bundling MSI). Server ini menyiapkan source yang siap di-pull dan di-build lokal.

## Yang diimplementasikan

### Tray always-on (ikon maskot + menu status)
- Ikon tray berbasis maskot `mascot_senang.png` (aset di-derive ke `src-tauri/icons/nexo-tray.{ico,png}` dan `tray-mascot-*.png`; tray runtime memakai `tray-mascot-32.png`).
- Menu tray:
  - **Status line** (dinamis, mengikuti state machine): Menghubungkan / Siaga / Menyimak / Memproses / Menjawab / Paused.
  - **Toggle Mic (on/off)**: “Matikan Mikrofon” / “Nyalakan Mikrofon” (pause/resume), mic tetap hot di background saat window disembunyikan/minimize ke tray.
  - **Buka Settings**: menampilkan jendela utama dan membuka SettingsSheet.
  - **Tampilkan Jendela**: restore/focus main window.
  - **Tampilkan/Sembunyikan Capsule**.
  - **Keluar**: konfirmasi dialog; saat konfirmasi, jalur shutdown bersih melepas mic (best-effort via backend) dan menutup aplikasi. (Integrasi `nexo-core` nyata harus mengaitkan close WS + unbind mic/audio di hook shutdown yang sama.)
- Tooltip tray menampilkan status + state mic (`Mic aktif` / `Mic nonaktif`). Klik kiri ikon tray toggle tampil/sembunyi jendela utama.

### Minimize-to-tray & capsule-on-minimize
- **Minimize** jendela utama → disembunyikan ke tray (ikon tray tetap, app tetap berjalan). **Close** jendela utama → hide ke tray (Quit eksplisit via tray/settings).
- **Capsule/notch (Dynamic Island style, always-on-top)** dapat tetap tampil saat minimize/ke tray, dikendalikan toggle **“Tampilkan capsule saat minimize/ke tray”** di Settings (default: aktif). Preferensi dipersist ke `ui-settings.json` di app config dir.
- Argumen start `--minimized` (dipakai autostart) memulai dengan main tersembunyi dan capsule mengikuti preferensi.

### Autostart saat login (toggle)
- Plugin `tauri-plugin-autostart` terpasang; toggle enable/disable dari Settings (Windows: Startup/autostart entry sesuai plugin). Status autostart dibaca saat Settings dibuka.
- **Verifikasi restart/login** harus dilakukan manual di Windows lokal (lihat checklist).

### Notifikasi (Windows)
- Notifikasi balon/toast best-effort saat **koneksi terputus** (transisi connected → disconnected, de-duplikasi) menginformasikan reconnect/backoff ditangani backend. Hook notifikasi error kritis tambahan tersedia untuk diaktifkan saat integrasi backend nyata mengemit error.

### Backend boundary & state
- State machine/snapshot tetap lewat boundary yang sama (TS `src/lib/backend/*`, Rust trait `NexoBackendEngine` + event `nexo://snapshot` + command `backend_*`), siap ditempel ke `nexo-core` Phase 1 (audio/VAD/WSS **out of scope** Phase 3). Mode saat ini: mock/simulasi.
- Token/URL diset lewat Settings; token tidak dilog di UI.

## File kunci yang diubah/ditambah (Phase 3)
- `src-tauri/src/lib.rs` — tray icon/menu dinamis, minimize-to-tray, capsule-on-minimize, autostart init (`--minimized`), dialog konfirmasi keluar + shutdown bersih, notifikasi disconnect, persistensi `ui-settings.json`, command/bridge UI.
- `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json` — plugin `dialog`/permissions, bump versi 0.1.1.
- `src-tauri/icons/nexo-tray.ico`, `nexo-tray.png`, `tray-mascot-*.png`, `mascot_senang_tray.png` — ikon tray dari maskot.
- `package.json` — dependensi `@tauri-apps/plugin-dialog`.
- `src/routes/+page.svelte` — listener `nexo://open-settings`, hint tray/background.
- `src/routes/capsule/+page.svelte` — status mic di capsule, toggle main/hide.
- `src/lib/components/SettingsSheet.svelte` — toggle autostart, toggle capsule-on-minimize, tombol Keluar (konfirmasi native), persistensi.
- `src/lib/stores/ui.ts` — store `capsuleOnMinimize`, `autostartEnabled`.

## Validasi yang dilakukan di host Linux (server ini)
- `npm install` (dependency graph up-to-date di repo).
- `npm run check` (`svelte-kit sync && svelte-check --tsconfig ./tsconfig.json`) → **0 errors, 0 warnings**.
- `npm run build` (SvelteKit/Vite static build) → **sukses** (output `build/`, termasuk `/` dan `/capsule`).
- `cargo check` (Tauri Rust) dan bundling MSI **tidak dijalankan** di Linux (butuh MSVC/WebView2/WiX/Windows target).

## Checklist verifikasi lokal Windows (user)
1. Pull branch `feat/windows-nexo-phase3-tray` dari `git@github.com:mrthx89/Nexo.git`.
2. Prasyarat: Node.js LTS, Rust stable MSVC (`x86_64-pc-windows-msvc`), Visual Studio Build Tools (C++ + Windows SDK), WebView2 Runtime, WiX (untuk MSI bundling).
3. Dari `clients/nexo-windows/nexo-ui/`:
   - `npm install`
   - `npm run tauri dev`
4. Uji tray/minimize:
   - Minimize jendela utama → jendela hilang, **ikon tray maskot** muncul & tetap.
   - Status tray/menu mengikuti state; toggle mic berfungsi; Buka Settings/Tampilkan Jendela/Tampilkan-Sembunyikan Capsule berfungsi.
   - (Jika toggle aktif) capsule/notch tetap tampil saat minimize.
   - Klik “Keluar” → konfirmasi muncul; keluar menutup bersih.
5. Uji autostart:
   - Aktifkan “Mulai otomatis saat login” di Settings, logout/login (atau restart) sekali → Nexo mulai (sesuai `--minimized`) dan tray/capsule mengikuti preferensi.
6. Build MSI (unsigned dev):
   - `npm run tauri build` → artefak di `src-tauri/target/release/bundle/` (lampirkan MSI/installer).
7. (Opsional, saat integrasi nyata) verifikasi notifikasi disconnect/error dan close WS + unbind mic end-to-end dengan `nexo-core`.

## Catatan keamanan & scope
- Tidak ada kredensial/token nyata di-commit. Token placeholder/redacted; input token tidak dilog.
- Audio/VAD/WSS dan detail UI kiosk berada di scope phase lain (`nexo-core` / Phase 2); Phase 3 menambah tray/minimize/autostart/notification & perilaku jendela.
