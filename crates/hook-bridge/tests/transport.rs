use commit_casino_bridge::{
    installer,
    sanitizer::{self, HookType},
    spool,
};
use std::{fs, path::Path};
fn setup() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("integration");
    installer::initialize(&root, commit_casino_bridge::DOCUMENTED_FIXTURE_VERSION).unwrap();
    (temp, root)
}
fn hook(name: &str) -> sanitizer::SanitizedHook {
    sanitizer::sanitize(&serde_json::to_vec(&serde_json::json!({"session_id":"session","hook_event_name":name,"prompt":"PRIVATE_CONTEXT"})).unwrap()).unwrap()
}
#[test]
fn contiguous_spool_replay_ack_and_unshown_recovery() {
    let (_temp, root) = setup();
    let start = spool::enqueue(&root, hook("SessionStart"), None).unwrap();
    let turn = spool::enqueue(&root, hook("UserPromptSubmit"), None).unwrap();
    assert_eq!(start.source_sequence, 1);
    assert_eq!(turn.source_sequence, 2);
    assert!(turn.turn_id.is_some());
    assert_eq!(spool::discover_sessions(&root).unwrap(), vec!["session"]);
    assert_eq!(spool::scan_inbox(&root, "session", 0).unwrap().len(), 2);
    assert!(spool::acknowledge(&root, &start, 0).is_err());
    spool::acknowledge(&root, &start, 1).unwrap();
    // Crash after DB commit, before deletion: retry skips already consumed sequence.
    assert_eq!(spool::scan_inbox(&root, "session", 2).unwrap().len(), 0);
    assert_eq!(
        spool::replayed_events(&root, "session", 2).unwrap().len(),
        1
    );
    spool::acknowledge(&root, &turn, 2).unwrap();
    assert!(spool::discover_sessions(&root).unwrap().is_empty());
    assert!(!spool::load_journal(&root, "session").unwrap().evidence_gap);
}
#[test]
fn publication_loss_blocks_watermark_and_poisoned_preallocation_is_visible() {
    let (_temp, root) = setup();
    let start = spool::enqueue(&root, hook("SessionStart"), None).unwrap();
    spool::enqueue(&root, hook("UserPromptSubmit"), None).unwrap();
    fs::remove_file(root.join("inbox").join(format!("{}.json", start.event_id))).unwrap();
    assert!(spool::scan_inbox(&root, "session", 0).unwrap().is_empty());
    spool::note_gap(&root);
    spool::enqueue(&root, hook("Stop"), None).unwrap();
    assert!(spool::load_journal(&root, "session").unwrap().evidence_gap);
}
#[test]
fn unknown_hooks_cannot_leak_inputs_or_create_success() {
    let (_temp, root) = setup();
    spool::enqueue(&root, hook("SessionStart"), None).unwrap();
    for entry in fs::read_dir(root.join("inbox")).unwrap() {
        assert!(!String::from_utf8(fs::read(entry.unwrap().path()).unwrap())
            .unwrap()
            .contains("PRIVATE_CONTEXT"));
    }
    assert!(sanitizer::sanitize(
        br#"{"session_id":"session","hook_event_name":"TaskCompleted","result":"passed"}"#
    )
    .is_err());
}
#[test]
fn bounded_lock_never_waits_for_another_hook() {
    let (_temp, root) = setup();
    let _lock = spool::lock_session(&root, "session").unwrap();
    let start = std::time::Instant::now();
    assert!(spool::enqueue(&root, hook("SessionStart"), None).is_err());
    assert!(start.elapsed() < std::time::Duration::from_millis(250));
}
#[test]
fn corrupt_signed_message_and_conflicting_sequence_fail_closed() {
    let (_temp, root) = setup();
    let event = spool::enqueue(&root, hook("SessionStart"), None).unwrap();
    let mut duplicate = event.clone();
    duplicate.event_id = uuid::Uuid::new_v4().to_string();
    let config = spool::load_config(&root).unwrap();
    spool::atomic_write(
        &root
            .join("inbox")
            .join(format!("{}.json", duplicate.event_id)),
        &spool::signed_bytes(&duplicate, &config.secret).unwrap(),
    )
    .unwrap();
    assert!(spool::scan_inbox(&root, "session", 0).is_err());
    let path = root.join("inbox").join(format!("{}.json", event.event_id));
    fs::write(path, b"not json").unwrap();
    assert!(spool::discover_sessions(&root).is_err());
}
#[test]
fn cross_session_link_cannot_be_rebound() {
    let (_temp, root) = setup();
    let config = spool::load_config(&root).unwrap();
    let link = "launch-link";
    spool::atomic_write(
        &root.join("links").join(format!("{link}.json")),
        &serde_json::to_vec(&spool::LaunchLink {
            installation_id: config.installation_id,
            created_at_ms: spool::now_ms(),
            session_id: None,
        })
        .unwrap(),
    )
    .unwrap();
    spool::enqueue(&root, hook("SessionStart"), Some(link)).unwrap();
    let mut other = hook("SessionStart");
    other.session_id = "other".into();
    assert!(spool::enqueue(&root, other, Some(link)).is_err());
    assert_eq!(
        spool::read_link(&root, link).unwrap().session_id.as_deref(),
        Some("session")
    );
}
#[test]
fn run_events_share_the_contiguous_authenticated_stream() {
    let (_temp, root) = setup();
    spool::enqueue(&root, hook("SessionStart"), None).unwrap();
    spool::enqueue(&root, hook("UserPromptSubmit"), None).unwrap();
    let _lock = spool::lock_session(&root, "session").unwrap();
    let mut journal = spool::load_journal(&root, "session").unwrap();
    let mut summary = commit_casino_bridge::verifier::RunSummary {
        run_id: "run".into(),
        session_id: "session".into(),
        turn_id: journal.turn_id.clone().unwrap(),
        suite_id: "unit".into(),
        runner: "vitest".into(),
        runner_version: "5.0.3".into(),
        exit_code: 0,
        passed: 1,
        failed: 0,
        skipped: 0,
        observed_failed_attempts: 0,
        started_sequence: 0,
        completed_sequence: 0,
        started_at_ms: spool::now_ms(),
        completed_at_ms: 0,
        integrity: "incomplete".into(),
    };
    spool::publish_verification(&root, "session", &mut journal, &mut summary, true).unwrap();
    summary.integrity = "valid".into();
    summary.completed_at_ms = spool::now_ms();
    spool::publish_verification(&root, "session", &mut journal, &mut summary, false).unwrap();
    let all = spool::scan_inbox(&root, "session", 0).unwrap();
    assert_eq!(all.len(), 4);
    assert_eq!(all[2].observation.kind, HookType::VerifiedRunStarted);
    assert_eq!(all[3].verification.as_ref().unwrap().completed_sequence, 4);
    assert!(!journal.evidence_gap);
}
#[test]
fn installer_preserves_unrelated_hooks_and_changed_entries() {
    let (_temp, root) = setup();
    let project = root.join("project with 空格");
    spool::private_dir(&project).unwrap();
    let helper = project.join("bridge.exe");
    spool::atomic_write(&helper, b"fixture").unwrap();
    let settings = project.join("settings.local.json");
    let unrelated =
        serde_json::json!({"matcher":"Read","hooks":[{"type":"command","command":"user-owned"}]});
    spool::atomic_write(
        &settings,
        &serde_json::to_vec(
            &serde_json::json!({"theme":"dark","hooks":{"PreToolUse":[unrelated.clone()]}}),
        )
        .unwrap(),
    )
    .unwrap();
    let manifest = root.join("manifests").join("project.json");
    installer::install(&settings, &helper, &manifest).unwrap();
    let mut changed: serde_json::Value =
        serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    changed["hooks"]["Stop"][0]["hooks"][0]["timeout"] = serde_json::json!(2);
    spool::atomic_write(&settings, &serde_json::to_vec(&changed).unwrap()).unwrap();
    assert_eq!(installer::disconnect(&manifest).unwrap(), vec!["Stop"]);
    let after: serde_json::Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(after["theme"], "dark");
    assert_eq!(after["hooks"]["PreToolUse"][0], unrelated);
    assert_eq!(after["hooks"]["Stop"][0]["hooks"][0]["timeout"], 2);
    assert!(manifest.exists());
}
#[test]
fn unknown_provider_version_and_reinstall_are_rejected() {
    let (_temp, root) = setup();
    assert!(
        installer::initialize(&root, commit_casino_bridge::DOCUMENTED_FIXTURE_VERSION).is_err()
    );
    let mut config = spool::load_config(&root).unwrap();
    config.provider_version = "unknown".into();
    spool::atomic_write(
        &root.join("config.json"),
        &serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    assert!(spool::enqueue(&root, hook("SessionStart"), None).is_err());
    assert!(installer::preview(Path::new("relative.exe")).is_err());
}
#[cfg(windows)]
#[test]
fn actual_hook_process_is_silent_and_exits_zero_on_missing_setup() {
    use std::process::{Command, Stdio};
    let temp = tempfile::tempdir().unwrap();
    let exe = env!("CARGO_BIN_EXE_commit-casino-bridge");
    let start = std::time::Instant::now();
    let mut child = Command::new(exe)
        .arg("hook")
        .env("LOCALAPPDATA", temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"session_id":"s","hook_event_name":"UserPromptSubmit","prompt":"PRIVATE"}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}
