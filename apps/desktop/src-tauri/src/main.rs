#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use casino_core::{Casino, CommitRequest, Error, Round};
use serde_json::Value;
use std::{
    ops::{Deref, DerefMut},
    sync::{Mutex, MutexGuard},
    time::Duration,
};
use tauri::{Emitter, Manager, State};
mod integration;
struct Service {
    casino: Mutex<Option<Casino>>,
    startup_error: Option<Error>,
}
struct CasinoGuard<'a>(MutexGuard<'a, Option<Casino>>);
impl Deref for CasinoGuard<'_> {
    type Target = Casino;
    fn deref(&self) -> &Casino {
        self.0.as_ref().expect("validated service guard")
    }
}
impl DerefMut for CasinoGuard<'_> {
    fn deref_mut(&mut self) -> &mut Casino {
        self.0.as_mut().expect("validated service guard")
    }
}
impl Service {
    fn lock(&self) -> Result<CasinoGuard<'_>> {
        let guard = self.casino.lock().map_err(|_| lock_error())?;
        if guard.is_none() {
            return Err(self.startup_error.clone().unwrap_or_else(lock_error));
        }
        Ok(CasinoGuard(guard))
    }
}
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
async fn get_snapshot(service: State<'_, Service>) -> Result<Value> {
    service.lock()?.snapshot()
}
#[tauri::command]
async fn commit_classic(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    request: CommitRequest,
) -> Result<Round> {
    let mut casino = service.lock()?;
    let round = casino.commit(request)?;
    publish(&app, &casino);
    let id = round.round_id.clone();
    let duration = round.presentation.duration_ms;
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(duration));
        let state = handle.state::<Service>();
        if let Ok(mut casino) = state.lock() {
            if casino.settle(&id).is_ok() {
                publish(&handle, &casino);
            }
        };
    });
    Ok(round)
}
#[tauri::command]
async fn acknowledge_presentation(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    round_id: String,
) -> Result<()> {
    let mut casino = service.lock()?;
    casino.acknowledge(&round_id)?;
    publish(&app, &casino);
    Ok(())
}
#[tauri::command]
async fn refill_wallet(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    request_id: String,
) -> Result<Value> {
    let mut casino = service.lock()?;
    let snapshot = casino.wallet_action(&request_id, false)?;
    publish(&app, &casino);
    Ok(snapshot)
}
#[tauri::command]
async fn reset_wallet(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    request_id: String,
) -> Result<Value> {
    let mut casino = service.lock()?;
    let snapshot = casino.wallet_action(&request_id, true)?;
    publish(&app, &casino);
    Ok(snapshot)
}
#[tauri::command]
async fn clear_history(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    request_id: String,
) -> Result<Value> {
    let mut casino = service.lock()?;
    let snapshot = casino.clear_history(&request_id)?;
    publish(&app, &casino);
    Ok(snapshot)
}
#[tauri::command]
async fn update_settings(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    patch: Value,
) -> Result<Value> {
    let mut casino = service.lock()?;
    let snapshot = casino.update_settings(patch)?;
    if let Some(mode) = snapshot["settings"]["windowMode"].as_str() {
        window_mode(&app, mode)
    }
    publish(&app, &casino);
    Ok(snapshot)
}

fn passive_launch(args: impl Iterator<Item = String>) -> bool {
    args.into_iter()
        .any(|a| a == "--auto-launch" || a == "--passive-agent-prompt")
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                if !passive_launch(args.into_iter()) {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
        }))
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            let opened = std::fs::create_dir_all(&directory)
                .map_err(|_| lock_error())
                .and_then(|_| Casino::open(directory.join("casino.sqlite3")));
            let (casino, startup_error) = match opened {
                Ok(casino) => (Some(casino), None),
                Err(error) => (None, Some(error)),
            };
            let mode = casino
                .as_ref()
                .and_then(|c| c.snapshot().ok())
                .and_then(|s| s["settings"]["windowMode"].as_str().map(str::to_owned))
                .unwrap_or_else(|| "standard".into());
            app.manage(Service {
                casino: Mutex::new(casino),
                startup_error,
            });
            integration::start_consumer(app.handle().clone());
            window_mode(app.handle(), &mode);
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                if !passive_launch(std::env::args()) {
                    window.set_focus()?;
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            commit_classic,
            acknowledge_presentation,
            refill_wallet,
            reset_wallet,
            update_settings,
            clear_history,
            integration::integration_status,
            integration::preview_integration,
            integration::install_integration,
            integration::disconnect_integration,
            integration::integration_check
        ])
        .run(tauri::generate_context!())
        .expect("Commit Casino window could not start. Saved data is preserved.");
}
