//! Narrow passive integration commands; no renderer-provided executable or shell arguments.
use super::{publish, Service};
use casino_core::{Casino, Error, PassiveObservation};
use commit_casino_bridge::{installer, spool, DOCUMENTED_FIXTURE_VERSION};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tauri::{Manager, State};
type Result<T> = std::result::Result<T, Error>;
fn error(message: impl Into<String>) -> Error {
    Error {
        code: "INTEGRATION_UNAVAILABLE",
        message: message.into(),
    }
}
fn bridge<T>(result: commit_casino_bridge::BridgeResult<T>) -> Result<T> {
    result.map_err(|e| error(format!("Passive integration unavailable: {e}")))
}
fn root() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| error("Windows app data directory unavailable."))?;
    let root = PathBuf::from(base).join("CommitCasino/integration");
    bridge(installer::reject_reparse_chain(&root))?;
    Ok(root)
}
fn helper(app: &tauri::AppHandle) -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|_| error("Application path unavailable."))?;
    let bundled = exe
        .parent()
        .ok_or_else(|| error("Application path unavailable."))?
        .join("commit-casino-bridge.exe");
    if bundled.is_file() {
        return Ok(bundled);
    }
    let resource = app
        .path()
        .resource_dir()
        .map_err(|_| error("Application resources unavailable."))?
        .join("commit-casino-bridge.exe");
    if resource.is_file() {
        return Ok(resource);
    }
    #[cfg(debug_assertions)]
    {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../crates/hook-bridge/target/release/commit-casino-bridge.exe");
        if path.is_file() {
            return path
                .canonicalize()
                .map_err(|_| error("Helper path unavailable."));
        }
    }
    Err(error(
        "The bundled hook helper is missing. Reinstall Commit Casino before connecting.",
    ))
}
fn settings(project: &str) -> Result<PathBuf> {
    let path = PathBuf::from(project);
    if !path.is_absolute() || !path.is_dir() {
        return Err(error("Choose an existing absolute Windows project folder."));
    }
    bridge(installer::reject_reparse_chain(&path))?;
    Ok(path.join(".claude/settings.local.json"))
}
fn manifest(root: &Path, path: &Path) -> PathBuf {
    root.join("manifests").join(format!(
        "{}.json",
        spool::session_key(&path.to_string_lossy())
    ))
}
fn detect_cli() -> Result<String> {
    // The command is fixed and read-only. Kill unresponsive CLI version checks after two seconds.
    let mut command = std::process::Command::new("claude");
    command
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| {
        error("Claude Code CLI was not found. Install its native Windows CLI first.")
    })?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(error("Claude CLI version check failed."));
                }
                break;
            }
            Ok(None) if started.elapsed() < Duration::from_secs(2) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error("Claude CLI version check timed out."));
            }
        }
    }
    let mut bytes = Vec::new();
    use std::io::Read;
    if let Some(stdout) = child.stdout.take() {
        stdout
            .take(4096)
            .read_to_end(&mut bytes)
            .map_err(|_| error("Claude version output unavailable."))?;
    }
    let text = String::from_utf8(bytes).map_err(|_| error("Invalid Claude version output."))?;
    let version = text.split_whitespace().next().unwrap_or("");
    if version.is_empty() {
        return Err(error("Claude version output unavailable."));
    }
    Ok(version.to_owned())
}
fn status(app: &tauri::AppHandle, service: &Service) -> Result<Value> {
    let root = root()?;
    let helper_ready = helper(app).is_ok();
    let config = spool::load_config(&root).ok();
    let connected = config
        .as_ref()
        .is_some_and(|c| c.provider_version == DOCUMENTED_FIXTURE_VERSION)
        && root.join("manifests").read_dir().is_ok_and(|mut d| {
            d.any(|e| {
                e.is_ok_and(|e| {
                    e.path().extension().is_some_and(|x| x == "json")
                        && !e
                            .path()
                            .file_stem()
                            .is_some_and(|x| x.to_string_lossy().ends_with(".backup"))
                })
            })
        });
    let mut casino = service.lock()?;
    casino.integration_connected(connected && helper_ready)?;
    Ok(
        json!({"available":helper_ready,"connected":connected&&helper_ready,"providerVersion":config.as_ref().map(|c|c.provider_version.as_str()),"supportedVersion":DOCUMENTED_FIXTURE_VERSION,"autoLaunch":config.as_ref().is_some_and(|c|c.auto_launch),"sessions":casino.passive_sessions()?,"message":if !helper_ready{"Bundled helper unavailable."}else if connected{"Passive local notifications connected. Agent predictions require separate verified evidence and remain disabled."}else{"Choose a project, preview the exact hooks, then connect. Gameplay works without Claude."}}),
    )
}
#[tauri::command]
pub async fn integration_status(
    app: tauri::AppHandle,
    service: State<'_, Service>,
) -> Result<Value> {
    status(&app, &service)
}
#[tauri::command]
pub async fn preview_integration(app: tauri::AppHandle, project_path: String) -> Result<Value> {
    let settings_path = settings(&project_path)?;
    let version = detect_cli()?;
    if version != DOCUMENTED_FIXTURE_VERSION {
        return Err(error(format!("Installed Claude version {version} is unsupported; supported fixture version is {DOCUMENTED_FIXTURE_VERSION}.")));
    }
    let entries = bridge(installer::preview(&helper(&app)?))?;
    Ok(
        json!({"projectPath":project_path,"settingsPath":settings_path,"entries":entries,"warning":"Project-local passive hooks only. Existing hooks are preserved and backed up. Restart Claude after connection; no prompts, replies, source, or transcripts are read. The local self-check does not replace a real lifecycle check."}),
    )
}
#[tauri::command]
pub async fn install_integration(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    project_path: String,
) -> Result<Value> {
    let selected = settings(&project_path)?;
    let version = detect_cli()?;
    if version != DOCUMENTED_FIXTURE_VERSION {
        return Err(error("Installed Claude CLI is not a supported version."));
    }
    let helper = helper(&app)?;
    let root = root()?;
    if !root.exists() {
        bridge(installer::initialize(&root, &version))?;
    }
    let config = bridge(spool::load_config(&root))?;
    if config.provider_version != version {
        return Err(error("Integration configuration version mismatch."));
    }
    bridge(installer::install(
        &selected,
        &helper,
        &manifest(&root, &selected),
    ))?;
    let mut config = config;
    config.desktop_executable = std::env::current_exe().ok();
    bridge(spool::atomic_write(
        &root.join("config.json"),
        &serde_json::to_vec_pretty(&config).map_err(|_| error("Configuration unavailable."))?,
    ))?;
    status(&app, &service)
}
#[tauri::command]
pub async fn disconnect_integration(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    project_path: String,
) -> Result<Value> {
    let selected = settings(&project_path)?;
    let root = root()?;
    let mismatches = bridge(installer::disconnect(&manifest(&root, &selected)))?;
    let mut value = status(&app, &service)?;
    if !mismatches.is_empty() {
        value["message"]=json!("Changed hook entries were preserved. Review them in Claude settings before reconnecting.");
    }
    Ok(value)
}
#[tauri::command]
pub async fn integration_check(
    app: tauri::AppHandle,
    service: State<'_, Service>,
) -> Result<Value> {
    let root = root()?;
    let config = bridge(spool::load_config(&root))?;
    // Sign and authenticate entirely in memory; never create a fake provider session or coding turn.
    let observation = bridge(commit_casino_bridge::sanitizer::sanitize(
        br#"{"session_id":"local-self-check","hook_event_name":"SessionStart"}"#,
    ))?;
    let event = spool::Envelope {
        version: 1,
        event_id: "local-self-check".into(),
        installation_id: config.installation_id.clone(),
        provider: "claude".into(),
        source_sequence: 1,
        observed_at_ms: spool::now_ms(),
        turn_id: None,
        observation,
        verification: None,
    };
    let bytes = bridge(spool::signed_bytes(&event, &config.secret))?;
    bridge(spool::authenticate(&bytes, &config, 0, spool::now_ms()))?;
    let mut value = status(&app, &service)?;
    value["message"]=json!("Local signature self-check passed. Start a real supported Claude session in the connected project to verify lifecycle notifications.");
    Ok(value)
}
pub fn start_consumer(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        use notify::Watcher;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut watcher = notify::recommended_watcher(move |_: notify::Result<notify::Event>| {
            let _ = tx.try_send(());
        })
        .ok();
        let mut watching = false;
        loop {
            if !watching {
                if let (Some(watcher), Ok(root)) = (&mut watcher, root()) {
                    let inbox = root.join("inbox");
                    if inbox.is_dir() {
                        watching = watcher
                            .watch(&inbox, notify::RecursiveMode::NonRecursive)
                            .is_ok();
                    }
                }
            }
            let _ = rx.recv_timeout(Duration::from_millis(750));
            let _ = consume(&app);
        }
    });
}
fn consume(app: &tauri::AppHandle) -> Result<()> {
    let root = root()?;
    let state = app.state::<Service>();
    let mut casino = state.lock()?;
    if ingest_inbox(&root, &mut casino)? {
        publish(app, &casino)
    }
    Ok(())
}
fn ingest_inbox(root: &Path, casino: &mut Casino) -> Result<bool> {
    if !root.exists() {
        return Ok(false);
    }
    let config = bridge(spool::load_config(root))?;
    bridge(spool::check_owned(&root.join("inbox")))?;
    let mut sessions = BTreeMap::<String, Vec<spool::Envelope>>::new();
    for entry in fs::read_dir(root.join("inbox"))
        .map_err(|_| error("Inbox unavailable."))?
        .take(spool::MAX_PENDING)
    {
        let path = entry.map_err(|_| error("Inbox unavailable."))?.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        if spool::check_owned(&path).is_err()
            || fs::metadata(&path).map_or(true, |m| m.len() > spool::MAX_MESSAGE)
        {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(event) = spool::authenticate(&bytes, &config, 0, spool::now_ms()) else {
            continue;
        };
        if path.file_stem().and_then(|s| s.to_str()) != Some(&event.event_id) {
            continue;
        }
        sessions
            .entry(event.observation.session_id.clone())
            .or_default()
            .push(event);
    }
    let mut changed = false;
    for (session, mut events) in sessions {
        events.sort_by_key(|e| e.source_sequence);
        if events
            .windows(2)
            .any(|e| e[0].source_sequence == e[1].source_sequence)
        {
            continue;
        }
        let mut watermark = casino.consumed_sequence(&config.installation_id, &session)?;
        for event in events {
            if event.source_sequence <= watermark {
                let _ = spool::acknowledge(root, &event, watermark);
                continue;
            }
            if event.source_sequence != watermark + 1 {
                break;
            }
            let kind = serde_json::to_value(&event.observation)
                .map_err(|_| error("Invalid observation."))?;
            changed |= casino.ingest_passive(PassiveObservation {
                installation: &event.installation_id,
                session: &session,
                event_id: &event.event_id,
                sequence: event.source_sequence,
                event_type: kind["type"].as_str().unwrap_or(""),
                turn_id: event.turn_id.as_deref(),
                observed_at_ms: event.observed_at_ms,
            })?;
            watermark = event.source_sequence;
            bridge(spool::acknowledge(root, &event, watermark))?;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_inbox_recovery_is_idempotent_and_cannot_touch_wallet() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("private-integration");
        installer::initialize(&root, DOCUMENTED_FIXTURE_VERSION).unwrap();
        let safe=commit_casino_bridge::sanitizer::sanitize(br#"{"session_id":"session-test","hook_event_name":"UserPromptSubmit","prompt":"NEVER PERSIST THIS","transcript_path":"NEVER PERSIST THIS"}"#).unwrap();
        let event = spool::enqueue(&root, safe, None).unwrap();
        let config = spool::load_config(&root).unwrap();
        let replay = spool::signed_bytes(&event, &config.secret).unwrap();
        let path = dir.path().join("casino.db");
        let mut casino = Casino::open(&path).unwrap();
        assert!(ingest_inbox(&root, &mut casino).unwrap());
        assert_eq!(casino.passive_sessions().unwrap()[0]["status"], "working");
        assert_eq!(casino.snapshot().unwrap()["wallet"]["balance"], 10000);
        // Crash after durable consume but before spool deletion: the same authenticated file is safe.
        spool::atomic_write(
            &root.join("inbox").join(format!("{}.json", event.event_id)),
            &replay,
        )
        .unwrap();
        drop(casino);
        let mut casino = Casino::open(&path).unwrap();
        assert!(!ingest_inbox(&root, &mut casino).unwrap());
        assert_eq!(
            casino
                .consumed_sequence(&config.installation_id, "session-test")
                .unwrap(),
            1
        );
        assert!(casino.reconcile().unwrap());
        assert_eq!(fs::read_dir(root.join("inbox")).unwrap().count(), 0);
        let bytes = fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("NEVER PERSIST THIS"));
    }
}

