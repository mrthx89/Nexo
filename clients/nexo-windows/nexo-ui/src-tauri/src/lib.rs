mod backend;

use backend::{BackendSettings, BackendSnapshot, MockBackend, NexoBackendEngine, NexoState};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Listener, Manager, Runtime, State, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;

const APP_SETTINGS_FILE: &str = "ui-settings.json";
const TRAY_ID: &str = "nexo-tray";

#[derive(Clone)]
struct AppBackend(Arc<dyn NexoBackendEngine>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppUiSettings {
    capsule_on_minimize: bool,
}

impl Default for AppUiSettings {
    fn default() -> Self {
        Self {
            capsule_on_minimize: true,
        }
    }
}

#[derive(Clone)]
struct AppSettingsState(Arc<Mutex<AppUiSettings>>);

impl AppSettingsState {
    fn get(&self) -> AppUiSettings {
        self.0.lock().expect("app settings mutex").clone()
    }
    fn set_capsule_on_minimize(&self, value: bool) -> AppUiSettings {
        let mut guard = self.0.lock().expect("app settings mutex");
        guard.capsule_on_minimize = value;
        guard.clone()
    }
}

#[derive(Clone)]
struct RuntimeFlags {
    allow_exit: Arc<AtomicBool>,
    tick_running: Arc<AtomicBool>,
    was_connected: Arc<AtomicBool>,
    notified_disconnected: Arc<AtomicBool>,
    cleaning_up: Arc<AtomicBool>,
}

impl Default for RuntimeFlags {
    fn default() -> Self {
        Self {
            allow_exit: Arc::new(AtomicBool::new(false)),
            tick_running: Arc::new(AtomicBool::new(true)),
            was_connected: Arc::new(AtomicBool::new(false)),
            notified_disconnected: Arc::new(AtomicBool::new(false)),
            cleaning_up: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    let dir = app.path().app_config_dir().ok()?;
    Some(dir.join(APP_SETTINGS_FILE))
}

fn load_ui_settings<R: Runtime>(app: &AppHandle<R>) -> AppUiSettings {
    let Some(path) = settings_path(app) else {
        return AppUiSettings::default();
    };
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AppUiSettings::default(),
    }
}

fn save_ui_settings<R: Runtime>(app: &AppHandle<R>, settings: &AppUiSettings) {
    let Some(path) = settings_path(app) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, content);
    }
}

#[tauri::command]
fn backend_get_snapshot(state: State<'_, AppBackend>) -> BackendSnapshot {
    state.0.snapshot()
}

#[tauri::command]
fn backend_set_mic(
    app: AppHandle,
    state: State<'_, AppBackend>,
    tray: State<'_, TrayState>,
    enabled: bool,
) -> Result<BackendSnapshot, String> {
    let snap = state.0.set_mic_enabled(enabled);
    let _ = app.emit("nexo://snapshot", &snap);
    let _ = update_tray(&app, &tray, &snap);
    Ok(snap)
}

#[tauri::command]
fn backend_toggle_pause(
    app: AppHandle,
    state: State<'_, AppBackend>,
    tray: State<'_, TrayState>,
) -> Result<BackendSnapshot, String> {
    let snap = state.0.toggle_pause();
    let _ = app.emit("nexo://snapshot", &snap);
    let _ = update_tray(&app, &tray, &snap);
    Ok(snap)
}

#[tauri::command]
fn backend_update_settings(
    app: AppHandle,
    state: State<'_, AppBackend>,
    settings: BackendSettings,
) -> Result<BackendSnapshot, String> {
    let snap = state.0.update_settings(settings);
    let _ = app.emit("nexo://snapshot", &snap);
    Ok(snap)
}

#[tauri::command]
fn app_get_ui_settings(state: State<'_, AppSettingsState>) -> AppUiSettings {
    state.get()
}

#[tauri::command]
fn app_set_capsule_on_minimize(
    app: AppHandle,
    state: State<'_, AppSettingsState>,
    enabled: bool,
) -> AppUiSettings {
    let updated = state.set_capsule_on_minimize(enabled);
    save_ui_settings(&app, &updated);
    updated
}

#[tauri::command]
fn app_show_main(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn app_quit(app: AppHandle) {
    request_quit(app);
}

struct TrayState {
    status_item: MenuItem<tauri::Wry>,
    toggle_item: MenuItem<tauri::Wry>,
}

fn mic_menu_label(snap: &BackendSnapshot) -> String {
    if snap.mic_enabled && snap.state != NexoState::Paused {
        "Matikan Mikrofon".to_string()
    } else {
        "Nyalakan Mikrofon".to_string()
    }
}

fn update_tray<R: Runtime>(
    app: &AppHandle<R>,
    tray: &TrayState,
    snap: &BackendSnapshot,
) -> tauri::Result<()> {
    let status_text = format!("Status: {}", snap.state.label());
    tray.status_item.set_text(status_text)?;
    tray.toggle_item.set_text(mic_menu_label(snap))?;
    if let Some(tray_icon) = app.tray_by_id(TRAY_ID) {
        let mic = if snap.mic_enabled && snap.state != NexoState::Paused {
            "Mic aktif"
        } else {
            "Mic nonaktif"
        };
        let _ = tray_icon.set_tooltip(Some(format!("Nexo — {} ({})", snap.state.label(), mic)));
    }
    Ok(())
}

fn tray_icon_image() -> tauri::image::Image<'static> {
    tauri::image::Image::from_bytes(include_bytes!("../icons/tray-mascot-32.png"))
        .unwrap_or_else(|_| tauri::image::Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("fallback tray icon"))
}

fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayState> {
    let status_item =
        MenuItem::with_id(app, "status", "Status: Menghubungkan", false, None::<&str>)?;
    let toggle_item = MenuItem::with_id(app, "toggle_mic", "Matikan Mikrofon", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Buka Settings", true, None::<&str>)?;
    let show_item = MenuItem::with_id(app, "show", "Tampilkan Jendela", true, None::<&str>)?;
    let show_capsule_item =
        MenuItem::with_id(app, "show_capsule", "Tampilkan Capsule", true, None::<&str>)?;
    let hide_capsule_item =
        MenuItem::with_id(app, "hide_capsule", "Sembunyikan Capsule", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &sep1,
            &toggle_item,
            &settings_item,
            &show_item,
            &show_capsule_item,
            &hide_capsule_item,
            &sep2,
            &quit_item,
        ],
    )?;

    let tray_state = TrayState {
        status_item,
        toggle_item,
    };

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tray_icon_image())
        .menu(&menu)
        .tooltip("Nexo — Menghubungkan")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle_mic" => {
                if let Some(backend) = app.try_state::<AppBackend>() {
                    let snap = backend.0.toggle_pause();
                    let _ = app.emit("nexo://snapshot", &snap);
                    if let Some(tray) = app.try_state::<TrayState>() {
                        let _ = update_tray(app, tray.inner(), &snap);
                    }
                }
            }
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }
            "settings" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
                let _ = app.emit("nexo://open-settings", ());
            }
            "show_capsule" => {
                if let Some(w) = app.get_webview_window("capsule") {
                    let _ = w.show();
                }
            }
            "hide_capsule" => {
                if let Some(w) = app.get_webview_window("capsule") {
                    let _ = w.hide();
                }
            }
            "quit" => {
                request_quit(app.clone());
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let visible = w.is_visible().unwrap_or(false);
                    if visible {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(tray_state)
}

fn send_notification<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) {
    // Best-effort: on Windows this shows a balloon/toast via plugin; ignore failures.
    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

fn clean_shutdown<R: Runtime>(app: &AppHandle<R>) {
    if let Some(flags) = app.try_state::<RuntimeFlags>() {
        if flags.cleaning_up.swap(true, Ordering::SeqCst) {
            return; // already shutting down
        }
        flags.tick_running.store(false, Ordering::SeqCst);
        flags.allow_exit.store(true, Ordering::SeqCst);
    } else {
        // fallback
    }

    // Best-effort: unbind mic / pause and prepare for clean WS close in real backend integration.
    if let Some(backend) = app.try_state::<AppBackend>() {
        let snap = backend.0.set_mic_enabled(false);
        let _ = app.emit("nexo://snapshot", &snap);
        if let Some(tray) = app.try_state::<TrayState>() {
            let _ = update_tray(app, tray.inner(), &snap);
        }
    }

    // Give UI/backend a brief moment conceptually not possible in sync callback; exit immediately.
    // Real nexo-core integration should hook WS close/unbind here before exit.
    app.exit(0);
}

fn request_quit<R: Runtime>(app: AppHandle<R>) {
    if let Some(flags) = app.try_state::<RuntimeFlags>() {
        if flags.cleaning_up.load(Ordering::SeqCst) {
            return;
        }
    }
    let app_for_yes = app.clone();
    app.dialog()
        .message("Keluar dari Nexo? Koneksi akan ditutup dan mikrofon dilepas dengan bersih.")
        .title("Keluar Nexo")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Keluar".to_string(), "Batal".to_string()))
        .show(move |confirmed| {
            if confirmed {
                clean_shutdown(&app_for_yes);
            }
        });
}

fn start_tick_loop(app: AppHandle, backend: Arc<dyn NexoBackendEngine>, flags: RuntimeFlags) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(900));
        loop {
            interval.tick().await;
            if !flags.tick_running.load(Ordering::SeqCst) {
                break;
            }
            if let Some(snap) = backend.tick() {
                // Connection/error notifications (best-effort, de-duplicated).
                if snap.connected {
                    flags.was_connected.store(true, Ordering::SeqCst);
                    flags.notified_disconnected.store(false, Ordering::SeqCst);
                } else if flags.was_connected.load(Ordering::SeqCst)
                    && !flags.notified_disconnected.swap(true, Ordering::SeqCst)
                {
                    send_notification(
                        &app,
                        "Nexo: Koneksi terputus",
                        "Koneksi ke layanan terputus. Nexo akan mencoba menyambung kembali (reconnect/backoff ditangani backend).",
                    );
                }

                let _ = app.emit("nexo://snapshot", &snap);
                if let Some(tray) = app.try_state::<TrayState>() {
                    let _ = update_tray(&app, tray.inner(), &snap);
                }
                if let Some(tray_icon) = app.tray_by_id(TRAY_ID) {
                    let mic = if snap.mic_enabled && snap.state != NexoState::Paused {
                        "Mic aktif"
                    } else {
                        "Mic nonaktif"
                    };
                    let _ = tray_icon
                        .set_tooltip(Some(format!("Nexo — {} ({})", snap.state.label(), mic)));
                }
            }
        }
    });
}

fn apply_minimize_to_tray<R: Runtime>(app: &AppHandle<R>, main: &tauri::WebviewWindow<R>) {
    // Hide main to tray on minimize; keep capsule visible based on user setting.
    let _ = main.hide();
    let _ = main.unminimize();
    if let Some(settings) = app.try_state::<AppSettingsState>() {
        let keep_capsule = settings.get().capsule_on_minimize;
        if let Some(capsule) = app.get_webview_window("capsule") {
            if keep_capsule {
                let _ = capsule.show();
            } else {
                let _ = capsule.hide();
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let backend: Arc<dyn NexoBackendEngine> = Arc::new(MockBackend::default());
    let app_backend = AppBackend(backend.clone());
    let flags = RuntimeFlags::default();
    let flags_for_setup = flags.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(app_backend)
        .manage(flags)
        .invoke_handler(tauri::generate_handler![
            backend_get_snapshot,
            backend_set_mic,
            backend_toggle_pause,
            backend_update_settings,
            app_get_ui_settings,
            app_set_capsule_on_minimize,
            app_show_main,
            app_quit
        ])
        .setup(move |app| {
            let loaded_settings = load_ui_settings(&app.handle());
            app.manage(AppSettingsState(Arc::new(Mutex::new(loaded_settings))));

            let tray_state = build_tray(&app.handle())?;
            app.manage(tray_state);
            start_tick_loop(app.handle().clone(), backend.clone(), flags_for_setup.clone());

            // Forward tray "open settings" to webview listeners.
            let handle = app.handle().clone();
            app.listen("nexo://open-settings", move |_| {
                if let Some(w) = handle.get_webview_window("main") {
                    let _ = w.emit("nexo://open-settings", ());
                }
            });

            let start_minimized = std::env::args().any(|a| a == "--minimized");

            if let Some(main) = app.get_webview_window("main") {
                let app_handle = app.handle().clone();
                let main_clone = main.clone();
                main.on_window_event(move |event| {
                    match event {
                        WindowEvent::CloseRequested { api, .. } => {
                            // Hide to tray instead of quitting (tray Quit exits explicitly).
                            let allow = app_handle
                                .try_state::<RuntimeFlags>()
                                .map(|f| f.allow_exit.load(Ordering::SeqCst))
                                .unwrap_or(false);
                            if !allow {
                                api.prevent_close();
                                let _ = main_clone.hide();
                            }
                        }
                        WindowEvent::Resized(_) => {
                            // Minimize-to-tray: when minimized, hide window (tray stays) and keep capsule per setting.
                            if main_clone.is_minimized().unwrap_or(false) {
                                apply_minimize_to_tray(&app_handle, &main_clone);
                            }
                        }
                        _ => {}
                    }
                });

                if start_minimized {
                    let _ = main.hide();
                }
            }

            if start_minimized {
                // Respect capsule-on-minimize preference when auto-starting minimized.
                let keep_capsule = app
                    .try_state::<AppSettingsState>()
                    .map(|s| s.get().capsule_on_minimize)
                    .unwrap_or(true);
                if let Some(capsule) = app.get_webview_window("capsule") {
                    if keep_capsule {
                        let _ = capsule.show();
                    } else {
                        let _ = capsule.hide();
                    }
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
