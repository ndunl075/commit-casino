//! This contract invokes the actual installed developer runner, never an AI model.
//! Turns are explicitly simulated hook metadata. CI runs it after npm ci.
use commit_casino_bridge::{installer, sanitizer, spool, verifier};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn node_exe() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .find(|p| p.is_absolute() && p.is_file())
    })
}
fn runner_cli(name: &str) -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|p| p.join("node_modules").join(name))
        .find(|p| p.is_file())
}
fn enqueue(root: &Path, name: &str, extra: serde_json::Value, link: &str) {
    let mut raw =
        serde_json::json!({"session_id":"simulated-runner-session","hook_event_name":name});
    raw.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    spool::enqueue(
        root,
        sanitizer::sanitize(&serde_json::to_vec(&raw).unwrap()).unwrap(),
        Some(link),
    )
    .unwrap();
}
fn actual_runner(runner: &str, cli: PathBuf, version: &str) {
    let node = node_exe().expect("Node required after npm ci");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("registered-runner-fixtures");
    fs::create_dir_all(&fixtures).unwrap();
    let private = tempfile::Builder::new()
        .prefix("space 空格-")
        .tempdir_in(fixtures)
        .unwrap();
    let root = private.path().join("CommitCasino").join("integration");
    installer::initialize(&root, commit_casino_bridge::DOCUMENTED_FIXTURE_VERSION).unwrap();
    let link = "explicit-test-launch";
    spool::atomic_write(
        &root.join("links").join(format!("{link}.json")),
        &serde_json::to_vec(&spool::LaunchLink {
            installation_id: spool::load_config(&root).unwrap().installation_id,
            created_at_ms: spool::now_ms(),
            session_id: None,
        })
        .unwrap(),
    )
    .unwrap();
    let runner_dir = private.path().join("runner");
    spool::private_dir(&runner_dir).unwrap();
    let (config_name, config) = if runner == "vitest" {
        (
            "vitest.config.mjs",
            "export default {test:{include:['*.test.ts']}};",
        )
    } else {
        (
            "playwright.config.mjs",
            "export default {testDir:'.',testMatch:'*.spec.ts',workers:1,outputDir:'./private-artifacts'};",
        )
    };
    fs::write(runner_dir.join(config_name), config).unwrap();
    let source_path = runner_dir.join(if runner == "vitest" {
        "fixture.test.ts"
    } else {
        "fixture.spec.ts"
    });
    let module = if runner == "vitest" {
        "vitest"
    } else {
        "@playwright/test"
    };
    let mut args = vec![
        cli.to_string_lossy().to_string(),
        if runner == "vitest" {
            "run".into()
        } else {
            "test".into()
        },
        "--config".into(),
        config_name.into(),
    ];
    if runner == "playwright" {
        args.push("--workers=1".into());
    }
    let suite = verifier::RegisteredSuite {
        suite_id: "explicit-suite".into(),
        runner: runner.into(),
        runner_version: version.into(),
        executable: node,
        args,
        version_args: vec![cli.to_string_lossy().to_string(), "--version".into()],
        working_directory: runner_dir,
    };
    spool::atomic_write(
        &root.join("suites").join("explicit-suite.json"),
        &serde_json::to_vec(&suite).unwrap(),
    )
    .unwrap();
    enqueue(&root, "SessionStart", serde_json::json!({}), link);
    for (index, should_pass) in [true, false].into_iter().enumerate() {
        let test_source=format!("import {{ test, expect }} from '{module}'; test('private assertion label', () => {{ expect(2+2).toBe({}); }});",if should_pass{4}else{5});
        fs::write(&source_path, test_source).unwrap();
        enqueue(
            &root,
            "UserPromptSubmit",
            serde_json::json!({"prompt":"never stored"}),
            link,
        );
        enqueue(
            &root,
            "PreToolUse",
            serde_json::json!({"tool_use_id":format!("shell-{index}"),"tool_name":"Bash"}),
            link,
        );
        assert!(
            verifier::verify_session(&root, "simulated-runner-session", "explicit-suite").is_err(),
            "dedicated verification rejects an enclosing active shell"
        );
        enqueue(
            &root,
            "PostToolUse",
            serde_json::json!({"tool_use_id":format!("shell-{index}"),"tool_name":"Bash"}),
            link,
        );
        enqueue(&root, "Stop", serde_json::json!({}), link);
        let status = std::process::Command::new(env!("CARGO_BIN_EXE_commit-casino-bridge"))
            .args([
                "verify-session",
                "simulated-runner-session",
                "explicit-suite",
            ])
            .env("LOCALAPPDATA", private.path())
            .env_remove("COMMIT_CASINO_LINK")
            .status()
            .unwrap();
        assert_eq!(
            status.code(),
            Some(if should_pass { 0 } else { 1 }),
            "explicit CLI resolves private session attribution and preserves child exit"
        );
    }
    enqueue(&root, "SessionEnd", serde_json::json!({}), link);
    let events = spool::scan_inbox(&root, "simulated-runner-session", 0).unwrap();
    let completed: Vec<_> = events
        .iter()
        .filter(|e| e.observation.kind == sanitizer::HookType::VerifiedRunCompleted)
        .collect();
    assert_eq!(completed.len(), 2);
    assert_eq!(
        completed[0].verification.as_ref().unwrap().integrity,
        "valid"
    );
    assert_eq!(completed[0].verification.as_ref().unwrap().passed, 1);
    assert_eq!(
        completed[1].verification.as_ref().unwrap().integrity,
        "valid"
    );
    assert_eq!(completed[1].verification.as_ref().unwrap().failed, 1);
    for entry in fs::read_dir(root.join("runs")).unwrap() {
        let dir = entry.unwrap().path();
        assert!(!dir.join("report.json").exists());
        assert!(
            !String::from_utf8(fs::read(dir.join("summary.json")).unwrap())
                .unwrap()
                .contains("private assertion label")
        );
    }
    assert!(
        !spool::load_journal(&root, "simulated-runner-session")
            .unwrap()
            .evidence_gap
    );
}
#[test]
#[ignore = "Run after npm ci: actual Vitest version and JSON contract"]
fn actual_vitest_preserves_success_and_failure_exits() {
    actual_runner(
        "vitest",
        runner_cli("vitest/vitest.mjs").expect("npm ci required"),
        verifier::VITEST_FIXTURE_VERSION,
    );
}
#[test]
#[ignore = "Run after npm ci: actual Playwright API test needs no browser or network"]
fn actual_playwright_preserves_success_and_failure_exits() {
    actual_runner(
        "playwright",
        runner_cli("@playwright/test/cli.js").expect("npm ci required"),
        verifier::PLAYWRIGHT_FIXTURE_VERSION,
    );
}
