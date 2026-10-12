#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use casino_core::{Casino, CommitRequest, Error, Round};
use serde_json::Value;
use std::{sync::Mutex, time::Duration};
use tauri::{Emitter, Manager, State};
struct Service(Mutex<Casino>);
type Result<T> = std::result::Result<T, Error>;
fn lock_error() -> Error {
    Error {
        code: "STORAGE_UNAVAILABLE",
        message: "Local service is unavailable. Restart to recover committed rounds.".into(),
    }
}
fn publish(app: &tauri::AppHandle, casino: &Casino) {
    if let Ok(snapshot) = casino.snapshot() {
        let _ = app.emit("casino://snapshot", snapshot);
    }
}
fn window_mode(app: &tauri::AppHandle, mode: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_fullscreen(mode == "fullscreen");
        if mode != "fullscreen" {
            let (width, height) = if mode == "compact" {
                (680., 760.)
            } else {
                (1080., 760.)
            };
            let _ = window.set_size(tauri::LogicalSize::new(width, height));
            let _ = window.center();
        }
    }
}
#[tauri::command]
fn get_snapshot(service: State<Service>) -> Result<Value> {
    service.0.lock().map_err(|_| lock_error())?.snapshot()
}
#[tauri::command]
fn commit_classic(
    app: tauri::AppHandle,
    service: State<Service>,
    request: CommitRequest,
) -> Result<Round> {
    let mut casino = service.0.lock().map_err(|_| lock_error())?;
    let round = casino.commit(request)?;
    publish(&app, &casino);
    let id = round.round_id.clone();
    let duration = round.presentation.duration_ms;
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(duration));
        let state = handle.state::<Service>();
        if let Ok(mut casino) = state.0.lock() {
            if casino.settle(&id).is_ok() {
                publish(&handle, &casino);
            }
        };
    });
    Ok(round)
}
#[tauri::command]
fn acknowledge_presentation(
    app: tauri::AppHandle,
    service: State<Service>,
    round_id: String,
) -> Result<()> {
    let mut casino = service.0.lock().map_err(|_| lock_error())?;
    casino.acknowledge(&round_id)?;
    publish(&app, &casino);
    Ok(())
}
#[tauri::command]
fn refill_wallet(
    app: tauri::AppHandle,
    service: State<Service>,
    request_id: String,
) -> Result<Value> {
    let mut casino = service.0.lock().map_err(|_| lock_error())?;
    let snapshot = casino.wallet_action(&request_id, false)?;
    publish(&app, &casino);
    Ok(snapshot)
}
#[tauri::command]
fn reset_wallet(
    app: tauri::AppHandle,
    service: State<Service>,
    request_id: String,
) -> Result<Value> {
    let mut casino = service.0.lock().map_err(|_| lock_error())?;
    let snapshot = casino.wallet_action(&request_id, true)?;
    publish(&app, &casino);
    Ok(snapshot)
}
#[tauri::command]
fn clear_history(
    app: tauri::AppHandle,
    service: State<Service>,
    request_id: String,
) -> Result<Value> {
    let mut casino = service.0.lock().map_err(|_| lock_error())?;
    let snapshot = casino.clear_history(&request_id)?;
    publish(&app, &casino);
    Ok(snapshot)
}
#[tauri::command]
fn update_settings(app: tauri::AppHandle, service: State<Service>, patch: Value) -> Result<Value> {
    let mut casino = service.0.lock().map_err(|_| lock_error())?;
    let snapshot = casino.update_settings(patch)?;
    if let Some(mode) = snapshot["settings"]["windowMode"].as_str() {
        window_mode(&app, mode)
    }
    publish(&app, &casino);
    Ok(snapshot)
}
fn main() {
    tauri::Builder::default()
  .plugin(tauri_plugin_single_instance::init(|app,args,_|{if let Some(window)=app.get_webview_window("main"){let _=window.show();if !args.iter().any(|a|a=="--auto-launch"){let _=window.unminimize();let _=window.set_focus();}}}))
  .setup(|app|{let directory=app.path().app_data_dir()?;std::fs::create_dir_all(&directory)?;let casino=Casino::open(directory.join("casino.sqlite3")).map_err(|e|std::io::Error::other(format!("{}: {}",e.code,e.message)))?;let snapshot=casino.snapshot().map_err(|e|std::io::Error::other(e.message))?;app.manage(Service(Mutex::new(casino)));window_mode(app.handle(),snapshot["settings"]["windowMode"].as_str().unwrap_or("standard"));if let Some(window)=app.get_webview_window("main"){window.show()?;if !std::env::args().any(|a|a=="--auto-launch"){window.set_focus()?;}}Ok(())})
  .invoke_handler(tauri::generate_handler![get_snapshot,commit_classic,acknowledge_presentation,refill_wallet,reset_wallet,update_settings,clear_history])
  .run(tauri::generate_context!()).expect("Commit Casino could not start. Saved data is preserved in the per-user application data directory.");
}
