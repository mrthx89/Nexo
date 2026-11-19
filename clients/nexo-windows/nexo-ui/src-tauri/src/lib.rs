mod backend;

use backend::{BackendSettings, BackendSnapshot, MockBackend, NexoBackendEngine};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime, State,
};

#[derive(Clone)]
struct AppBackend(Arc<dyn NexoBackendEngine>);

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

struct TrayState {
    status_item: MenuItem<tauri::Wry>,
    toggle_item: MenuItem<tauri::Wry>,
}

fn update_tray<R: Runtime>(app: &AppHandle<R>, tray: &TrayState, snap: &BackendSnapshot) -> tauri::Result<()> {
    let status_text = format!("Status: {}", snap.state.label());
    tray.status_item.set_text(status_text)?;
    let mic_text = if snap.mic_enabled && snap.state != backend::NexoState::Paused {
        "Pause Mic"
    } else {
        "Resume Mic"
    };
    tray.toggle_item.set_text(mic_text)?;
    if let Some(tray_icon) = app.tray_by_id("nexo-tray") {
        let _ = tray_icon.set_tooltip(Some(format!("Nexo — {}", snap.state.label())));
    }
    Ok(())
}

fn build_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<TrayState> {
    let status_item = MenuItem::with_id(app, "status", "Status: Menghubungkan", false, None::<&str>)?;
    let toggle_item = MenuItem::with_id(app, "toggle_mic", "Pause Mic", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Buka Settings", true, None::<&str>)?;
    let show_item = MenuItem::with_id(app, "show", "Tampilkan Jendela", true, None::<&str>)?;
    let show_capsule_item = MenuItem::with_id(app, "show_capsule", "Tampilkan Capsule", true, None::<&str>)?;
    let hide_capsule_item = MenuItem::with_id(app, "hide_capsule", "Sembunyikan Capsule", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &sep1,
            &toggle_item,
            &show_item,
            &settings_item,
            &show_capsule_item,
            &hide_capsule_item,
            &sep2,
            &quit_item,
        ],
    )?;

    let tray_state = TrayState { status_item, toggle_item };

    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::with_id("nexo-tray")
        .menu(&menu)
        .tooltip("Nexo — Menghubungkan")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle_mic" => {
                let backend = app.state::<AppBackend>().inner().clone();
                let tray = app.state::<TrayState>();
                let snap = backend.0.toggle_pause();
                let _ = app.emit("nexo://snapshot", &snap);
                let _ = update_tray(app, tray.inner(), &snap);
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
                // Clean shutdown path; real backend integration must close WS + unbind mic here.
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
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
        });
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(tray_state)
}

fn start_tick_loop(app: AppHandle, backend: Arc<dyn NexoBackendEngine>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(900));
        loop {
            interval.tick().await;
            if let Some(snap) = backend.tick() {
                let _ = app.emit("nexo://snapshot", &snap);
                if let Some(tray) = app.try_state::<TrayState>() {
                    let _ = update_tray(&app, tray.inner(), &snap);
                }
                if let Some(tray_icon) = app.tray_by_id("nexo-tray") {
                    let _ = tray_icon.set_tooltip(Some(format!("Nexo — {}", snap.state.label())));
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let backend: Arc<dyn NexoBackendEngine> = Arc::new(MockBackend::default());
    let app_backend = AppBackend(backend.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(app_backend)
        .invoke_handler(tauri::generate_handler![
            backend_get_snapshot,
            backend_set_mic,
            backend_toggle_pause,
            backend_update_settings
        ])
        .setup(move |app| {
            let tray_state = build_tray(&app.handle())?;
            app.manage(tray_state);
            start_tick_loop(app.handle().clone(), backend.clone());

            // Open settings event from tray -> forward to main webview (UI listens).
            let handle = app.handle().clone();
            app.listen("nexo://open-settings", move |_| {
                if let Some(w) = handle.get_webview_window("main") {
                    let _ = w.emit("nexo://open-settings", ());
                }
            });

            // Minimize-to-tray friendly behavior: hide main on close request? keep running.
            if let Some(main) = app.get_webview_window("main") {
                let main_clone = main.clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Hide to tray instead of quitting (tray Quit exits explicitly).
                        api.prevent_close();
                        let _ = main_clone.hide();
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
