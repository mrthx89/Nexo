use nexo_core::config::{app_data_dir, Config, DEFAULT_SERVER_URL, TOKEN_PLACEHOLDER};
use nexo_core::credential_store::DeviceCredentialStore;
use nexo_core::engine::{spawn_engine, EngineCommand, EngineConfig, EngineHandle, EngineSnapshot};
use nexo_core::iot_http::IotHttpClient;
use nexo_core::iot_wiz::{self, DiscoveredBulb, WizControlParams};
use serde_json::Value;
use std::path::PathBuf;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, State, WindowEvent};

struct AppState {
    engine: EngineHandle,
    cred_store: DeviceCredentialStore,
}

fn resolve_vad_model(explicit: Option<PathBuf>) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(p) = explicit {
        candidates.push(p);
    }
    candidates.push(app_data_dir().join("silero-v6.2.3.onnx"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("silero-v6.2.3.onnx"));
            candidates.push(dir.join("resources/silero-v6.2.3.onnx"));
        }
    }
    candidates.push(PathBuf::from(
        "../../nexo-android/app/src/main/assets/vad/silero-v6.2.3.onnx",
    ));
    candidates.push(PathBuf::from(
        "../../../nexo-android/app/src/main/assets/vad/silero-v6.2.3.onnx",
    ));
    candidates.into_iter().find(|p| p.is_file())
}

#[tauri::command]
async fn get_snapshot(state: State<'_, AppState>) -> Result<EngineSnapshot, String> {
    Ok(state.engine.snapshot().await)
}

#[tauri::command]
fn start_voice(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.send(EngineCommand::StartVoice).map_err(|e| e.to_string())
}

#[tauri::command]
fn stop_voice(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.send(EngineCommand::StopVoice).map_err(|e| e.to_string())
}

#[tauri::command]
fn finish_utterance(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::FinishUtterance)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn start_manual_utterance(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::StartManualUtterance)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_automatic_mode(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::SetAutomatic(enabled))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_credentials(
    state: State<'_, AppState>,
    server_url: String,
    token: Option<String>,
    vad_model: Option<String>,
) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::UpdateConfig {
            server_url,
            token: token.filter(|t| !t.trim().is_empty()),
            vad_model: vad_model.filter(|p| !p.trim().is_empty()).map(PathBuf::from),
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_credentials(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.send(EngineCommand::ClearToken).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_recording_sheet(state: State<'_, AppState>, diagnostic_only: bool) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::OpenRecordingSheet { diagnostic_only })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn close_recording_sheet(state: State<'_, AppState>, resume: bool) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::CloseRecordingSheet { resume })
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn start_enrollment_or_diagnostic(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::StartEnrollmentOrDiagnostic)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_enrollment(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::SaveEnrollment)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn unenroll_voiceprint(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::UnenrollVoiceprint)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn play_diagnostic(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::PlayDiagnostic)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_diagnostic(state: State<'_, AppState>) -> Result<(), String> {
    state
        .engine
        .send(EngineCommand::DeleteDiagnostic)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn load_cached_bulbs() -> Vec<DiscoveredBulb> {
    iot_wiz::load_cached_bulbs(&iot_wiz::default_bulb_cache_path())
}

#[tauri::command]
async fn scan_wiz_bulbs() -> Result<Vec<DiscoveredBulb>, String> {
    tokio::task::spawn_blocking(|| iot_wiz::scan_bulbs(&iot_wiz::default_bulb_cache_path(), 2500))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn control_wiz_bulb(params: WizControlParams) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || iot_wiz::set_pilot(&params))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

async fn make_iot_client(state: &AppState) -> Result<IotHttpClient, String> {
    let snap = state.engine.snapshot().await;
    let tok = state
        .cred_store
        .read_token(&snap.server_url, &snap.device_id)
        .ok_or_else(|| "Token perangkat belum diatur. Buka Pengaturan terlebih dahulu.".to_string())?;
    IotHttpClient::from_wss(&snap.server_url, &tok, &snap.device_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn iot_status(state: State<'_, AppState>) -> Result<Value, String> {
    let client = make_iot_client(&state).await?;
    client.iot_status().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn iot_control(state: State<'_, AppState>, payload: Value) -> Result<Value, String> {
    let client = make_iot_client(&state).await?;
    client.iot_control(payload).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn ac_status(state: State<'_, AppState>, serial: Option<String>) -> Result<Value, String> {
    let client = make_iot_client(&state).await?;
    client
        .ac_status(serial.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn ac_devices(state: State<'_, AppState>) -> Result<Value, String> {
    let client = make_iot_client(&state).await?;
    client.ac_devices().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn ac_request_otp(state: State<'_, AppState>, phone: String) -> Result<Value, String> {
    let client = make_iot_client(&state).await?;
    client.ac_request_otp(&phone).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn ac_verify_otp(
    state: State<'_, AppState>,
    phone: String,
    otp_code: String,
) -> Result<Value, String> {
    let client = make_iot_client(&state).await?;
    client
        .ac_verify_otp(&phone, &otp_code)
        .await
        .map_err(|e| e.to_string())
}

pub fn run() {
    tracing_subscriber::fmt::try_init().ok();

    let cfg = Config::load().unwrap_or_else(|_| Config {
        server_url: DEFAULT_SERVER_URL.into(),
        device_id: "nexo_windows".into(),
        bearer_token: TOKEN_PLACEHOLDER.into(),
        vad_model: None,
    });
    let cred_store = DeviceCredentialStore::default_store();
    let server_url = cred_store
        .saved_server_url()
        .unwrap_or_else(|| cfg.server_url.clone());
    let device_id = cred_store
        .load_or_create_device_id()
        .unwrap_or_else(|_| cfg.device_id.clone());
    let token = if cfg.has_real_token() {
        Some(cfg.bearer_token.clone())
    } else {
        cred_store.read_token(&server_url, &device_id)
    };
    let vad_model = resolve_vad_model(cfg.vad_model.clone());

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let engine = rt.block_on(async {
        spawn_engine(EngineConfig {
            server_url,
            device_id,
            token,
            vad_model,
        })
    });
    let _rt_guard = rt.enter();

    let engine_for_events = engine.clone();
    let engine_for_tray = engine.clone();
    let engine_for_exit = engine.clone();

    tauri::Builder::default()
        .manage(AppState { engine, cred_store })
        .setup(move |app| {
            let show_item = MenuItem::with_id(app, "show", "Tampilkan Nexo", true, None::<&str>)?;
            let toggle_item =
                MenuItem::with_id(app, "toggle_mic", "Mulai / Berhenti Mikrofon", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Keluar", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &toggle_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Nexo Appliance Desktop");
            if let Some(icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(icon);
            }
            tray_builder
                .on_menu_event(move |app_handle, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app_handle.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "toggle_mic" => {
                        let eng = engine_for_tray.clone();
                        tauri::async_runtime::spawn(async move {
                            let snap = eng.snapshot().await;
                            let cmd = if snap.voice_enabled {
                                EngineCommand::StopVoice
                            } else {
                                EngineCommand::StartVoice
                            };
                            let _ = eng.send(cmd);
                        });
                    }
                    "quit" => {
                        let _ = engine_for_exit.send(EngineCommand::Shutdown);
                        app_handle.exit(0);
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
                        if let Some(w) = tray.app_handle().get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            let app_handle = app.handle().clone();
            let mut rx = engine_for_events.subscribe();
            tauri::async_runtime::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            let _ = app_handle.emit("nexo-event", &ev);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Keep resident in System Tray when minimized/closed
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            start_voice,
            stop_voice,
            finish_utterance,
            start_manual_utterance,
            set_automatic_mode,
            save_credentials,
            clear_credentials,
            open_recording_sheet,
            close_recording_sheet,
            start_enrollment_or_diagnostic,
            save_enrollment,
            unenroll_voiceprint,
            play_diagnostic,
            delete_diagnostic,
            load_cached_bulbs,
            scan_wiz_bulbs,
            control_wiz_bulb,
            iot_status,
            iot_control,
            ac_status,
            ac_devices,
            ac_request_otp,
            ac_verify_otp,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Nexo Desktop");
}
