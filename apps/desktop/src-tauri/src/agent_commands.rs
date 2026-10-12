use crate::{integration, publish, Service};
use agent_core::{CheckKind, Integrity, VerificationRun};
use casino_core::{
    agent::{AgentObservation, ArmAgentRequest, ArmAuthority},
    Casino, Error,
};
use commit_casino_bridge::{spool, verifier, DOCUMENTED_FIXTURE_VERSION};
use serde_json::{json, Value};
use std::path::Path;
use tauri::State;
type Result<T> = std::result::Result<T, Error>;
fn error(message: impl Into<String>) -> Error {
    Error {
        code: "INTEGRATION_UNAVAILABLE",
        message: message.into(),
    }
}
fn checked<T>(result: commit_casino_bridge::BridgeResult<T>) -> Result<T> {
    result.map_err(|_| error("Verified agent evidence unavailable. Check the integration setup and registered required suites."))
}
fn policy(root: &Path) -> Result<Vec<String>> {
    Ok(checked(verifier::load_required_suites(root))?
        .into_iter()
        .map(|s| s.suite_id)
        .collect())
}
fn eligible(root: &Path, installation: &str, session: &Value, casino: &Casino) -> Result<()> {
    let id = session["sessionId"]
        .as_str()
        .ok_or_else(|| error("Invalid session."))?;
    if !matches!(session["status"].as_str(), Some("idle" | "stopped")) {
        return Err(error(
            "Arm only while the connected session is idle, before its next prompt.",
        ));
    }
    let journal = checked(spool::load_journal(root, id))?;
    if journal.evidence_gap
        || journal.ended
        || (!journal.stopped && journal.turn_id.is_some())
        || !journal.tool_turns.is_empty()
    {
        return Err(error(
            "Session has activity or uncertain boundary coverage.",
        ));
    }
    if journal.sequence != casino.consumed_sequence(installation, id)? {
        return Err(error("Waiting for the native inbox to drain."));
    }
    if casino.agent_has_active_runs(installation, id)? {
        return Err(error("A prior verification run is still unresolved."));
    }
    let link=journal.link_id.ok_or_else(||error("Start this session through commit-casino-bridge launch-claude for verified attribution."))?;
    let launch = checked(spool::read_link(root, &link))?;
    if launch.installation_id != installation || launch.session_id.as_deref() != Some(id) {
        return Err(error("Verified launcher link does not match this session."));
    }
    if casino.agent_wagers()?.iter().any(|w| {
        w.session_id == id
            && w.installation_id == installation
            && matches!(w.status.as_str(), "armed" | "bound")
    }) {
        return Err(error("This session already has an unresolved prediction."));
    }
    Ok(())
}
fn status(root: &Path, casino: &mut Casino) -> Result<Value> {
    let config = spool::load_config(root).ok();
    let required = policy(root);
    let enabled = config
        .as_ref()
        .is_some_and(|c| c.provider_version == DOCUMENTED_FIXTURE_VERSION)
        && required.is_ok();
    casino.agent_ready(enabled)?;
    let mut sessions = vec![];
    for session in casino.passive_sessions()? {
        let result = config
            .as_ref()
            .ok_or_else(|| error("Passive integration not configured."))
            .and_then(|c| eligible(root, &c.installation_id, &session, casino));
        sessions.push(json!({"sessionId":session["sessionId"],"eligible":enabled&&result.is_ok(),"reason":if !enabled{required.as_ref().err().map(|e|e.message.as_str()).unwrap_or("Supported passive integration required.")}else{result.as_ref().err().map(|e|e.message.as_str()).unwrap_or("Ready to predict the next observed turn.")}}));
    }
    let snapshot = casino.snapshot()?;
    let mut wagers = serde_json::to_value(casino.agent_wagers()?)
        .map_err(|_| error("Prediction history unavailable."))?;
    if let Some(wagers) = wagers.as_array_mut() {
        for wager in wagers {
            if let Some(object) = wager.as_object_mut() {
                object.remove("linkId");
            }
        }
    }
    Ok(
        json!({"revision":snapshot["revision"],"enabled":enabled,"message":if enabled{"Experimental local evidence mode. Arm before the next prompt. Fixed fictional-chip odds 1:1. Unknown, cancellation and fifteen-minute timeouts refund the stake. Local registered checks describe observations, not code correctness."}else{"Verified launcher attribution and registered required test suites must be configured first. Run commit-casino-bridge register-suite, require-suites, and launch-claude. Classic Roulette is always available."},"sessions":sessions,"wagers":wagers,"stats":snapshot["agentStats"]}),
    )
}
#[tauri::command]
pub async fn agent_status(app: tauri::AppHandle, service: State<'_, Service>) -> Result<Value> {
    let root = integration::root()?;
    let mut casino = service.lock()?;
    let ingested = integration::ingest_inbox(&root, &mut casino)?;
    let expired = if integration::pending_hook_in_flight(&root) {
        false
    } else {
        casino.expire_agent(spool::now_ms())?
    };
    let changed = ingested | expired;
    let result = status(&root, &mut casino)?;
    if changed {
        publish(&app, &casino)
    }
    Ok(result)
}
#[tauri::command]
pub async fn arm_agent(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    request: ArmAgentRequest,
) -> Result<Value> {
    let root = integration::root()?;
    {
        let mut casino = service.lock()?;
        if casino.agent_arm_replay(&request)?.is_some() {
            return status(&root, &mut casino);
        }
    }
    let config = checked(spool::load_config(&root))?;
    if config.provider_version != DOCUMENTED_FIXTURE_VERSION {
        return Err(error("Unsupported passive integration profile."));
    }
    let required = policy(&root)?;
    if !commit_casino_bridge::sanitizer::opaque(&request.session_id) {
        return Err(error("Invalid session ID."));
    }
    // Exact helper lock shared with prompt enqueue: no preexisting unseen prompt can accept a prediction.
    let _boundary_lock = checked(spool::lock_session(&root, &request.session_id))?;
    let mut casino = service.lock()?;
    integration::ingest_inbox(&root, &mut casino)?;
    let session = casino
        .passive_sessions()?
        .into_iter()
        .find(|s| s["sessionId"] == request.session_id)
        .ok_or_else(|| error("Observe a fresh connected session before arming."))?;
    eligible(&root, &config.installation_id, &session, &casino)?;
    let journal = checked(spool::load_journal(&root, &request.session_id))?;
    let authority = ArmAuthority {
        installation_id: config.installation_id,
        link_id: journal
            .link_id
            .ok_or_else(|| error("Verified launcher link missing."))?,
        watermark: journal.sequence,
        required_suite_ids: required,
        observed_at_ms: spool::now_ms(),
    };
    casino.arm_agent(request, authority)?;
    let result = status(&root, &mut casino)?;
    publish(&app, &casino);
    Ok(result)
}
#[tauri::command]
pub async fn cancel_agent(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    request_id: String,
    wager_id: String,
) -> Result<Value> {
    let root = integration::root()?;
    let session = service.lock()?.agent_wager(&wager_id)?.session_id;
    let _boundary_lock = checked(spool::lock_session(&root, &session))?;
    let mut casino = service.lock()?;
    integration::ingest_inbox(&root, &mut casino)?;
    if integration::pending_hook_in_flight(&root) {
        return Err(error(
            "Wait for the pending provider hook before canceling.",
        ));
    }
    casino.void_agent(&request_id, &wager_id, true)?;
    let result = status(&root, &mut casino)?;
    publish(&app, &casino);
    Ok(result)
}
#[tauri::command]
pub async fn void_agent(
    app: tauri::AppHandle,
    service: State<'_, Service>,
    request_id: String,
    wager_id: String,
) -> Result<Value> {
    let root = integration::root()?;
    let session = service.lock()?.agent_wager(&wager_id)?.session_id;
    let _boundary_lock = checked(spool::lock_session(&root, &session))?;
    let mut casino = service.lock()?;
    integration::ingest_inbox(&root, &mut casino)?;
    casino.void_agent(&request_id, &wager_id, false)?;
    let result = status(&root, &mut casino)?;
    publish(&app, &casino);
    Ok(result)
}
pub(super) fn normalize_observation(root: &Path, event: &spool::Envelope) -> AgentObservation {
    let journal = spool::load_journal(root, &event.observation.session_id).ok();
    let verification = event.verification.as_ref().map(|r| VerificationRun {
        run_id: r.run_id.clone(),
        session_id: r.session_id.clone(),
        turn_id: r.turn_id.clone(),
        suite_id: r.suite_id.clone(),
        kind: CheckKind::Test,
        started_sequence: r.started_sequence,
        completed_sequence: r.completed_sequence,
        passed: r.passed,
        failed: r.failed,
        skipped: r.skipped,
        observed_failed_attempts: r.observed_failed_attempts,
        exit_code: r.exit_code,
        integrity: match r.integrity.as_str() {
            "valid" => Integrity::Valid,
            "conflicting" => Integrity::Conflicting,
            _ => Integrity::Incomplete,
        },
        supported: match r.runner.as_str() {
            "vitest" => r.runner_version == verifier::VITEST_FIXTURE_VERSION,
            "playwright" => r.runner_version == verifier::PLAYWRIGHT_FIXTURE_VERSION,
            _ => false,
        },
        canceled: r.exit_code < 0,
    });
    let effect = event.observation.effect.as_ref().map(|e| match e {
        commit_casino_bridge::sanitizer::ToolEffect::Read => "read".to_owned(),
        _ => "possible-mutation".to_owned(),
    });
    AgentObservation {
        tool_id: event.observation.tool_id.clone(),
        effect,
        verification,
        journal_gap: journal.as_ref().is_none_or(|j| j.evidence_gap),
        link_id: journal.and_then(|j| j.link_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use commit_casino_bridge::{installer, sanitizer};
    use std::{fs, path::PathBuf};
    fn enqueue(root: &Path, name: &str, extra: Value) {
        let mut value = json!({"session_id":"native-agent-session","hook_event_name":name});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        spool::enqueue(
            root,
            sanitizer::sanitize(&serde_json::to_vec(&value).unwrap()).unwrap(),
            Some("native-test-link"),
        )
        .unwrap();
    }
    fn fixture() -> (tempfile::TempDir, PathBuf, Casino) {
        let parent =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/agent-pipeline-fixtures");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("native 空格-")
            .tempdir_in(parent)
            .unwrap();
        let root = temp.path().join("private");
        installer::initialize(&root, DOCUMENTED_FIXTURE_VERSION).unwrap();
        let config = spool::load_config(&root).unwrap();
        spool::atomic_write(
            &root.join("links/native-test-link.json"),
            &serde_json::to_vec(&spool::LaunchLink {
                installation_id: config.installation_id,
                created_at_ms: spool::now_ms(),
                session_id: None,
            })
            .unwrap(),
        )
        .unwrap();
        let casino = Casino::open(temp.path().join("wallet.db")).unwrap();
        (temp, root, casino)
    }
    #[test]
    fn unseen_prompt_and_pending_hook_cannot_accept_stake() {
        let (_temp, root, mut casino) = fixture();
        enqueue(&root, "SessionStart", json!({}));
        integration::ingest_inbox(&root, &mut casino).unwrap();
        let config = spool::load_config(&root).unwrap();
        let idle = casino.passive_sessions().unwrap()[0].clone();
        assert!(eligible(&root, &config.installation_id, &idle, &casino).is_ok());
        let ticket = spool::begin_hook(&root).unwrap();
        assert!(eligible(&root, &config.installation_id, &idle, &casino).is_err());
        spool::complete_hook(&root, &ticket).unwrap();
        enqueue(&root, "UserPromptSubmit", json!({"prompt":"discard this"}));
        assert!(eligible(&root, &config.installation_id, &idle, &casino).is_err());
        assert_eq!(casino.snapshot().unwrap()["wallet"]["balance"], 10000);
    }
    #[test]
    #[ignore = "Run after npm ci: real Vitest process through authenticated hooks and SQLite authority"]
    fn actual_agent_runner_wallet_pipeline() {
        let (temp, root, mut casino) = fixture();
        let node = std::env::var_os("PATH")
            .and_then(|p| {
                std::env::split_paths(&p)
                    .map(|p| p.join(if cfg!(windows) { "node.exe" } else { "node" }))
                    .find(|p| p.is_file())
            })
            .expect("Node required after npm ci");
        let cli = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .map(|p| p.join("node_modules/vitest/vitest.mjs"))
            .find(|p| p.is_file())
            .expect("Vitest required after npm ci");
        let runner = temp.path().join("registered-tests");
        spool::private_dir(&runner).unwrap();
        fs::write(
            runner.join("vitest.config.mjs"),
            "export default {test:{include:['*.test.ts']}};",
        )
        .unwrap();
        fs::write(runner.join("fixture.test.ts"),"import {test,expect} from 'vitest';test('private name must never persist',()=>expect(2+2).toBe(4));").unwrap();
        let suite = verifier::RegisteredSuite {
            suite_id: "native-tests".into(),
            runner: "vitest".into(),
            runner_version: verifier::VITEST_FIXTURE_VERSION.into(),
            executable: node,
            args: vec![
                cli.to_string_lossy().into(),
                "run".into(),
                "--config".into(),
                "vitest.config.mjs".into(),
            ],
            version_args: vec![cli.to_string_lossy().into(), "--version".into()],
            working_directory: runner,
        };
        spool::atomic_write(
            &root.join("suites/native-tests.json"),
            &serde_json::to_vec(&suite).unwrap(),
        )
        .unwrap();
        spool::atomic_write(
            &root.join("required-suites.json"),
            &serde_json::to_vec(&json!({"suiteIds":["native-tests"]})).unwrap(),
        )
        .unwrap();
        enqueue(&root, "SessionStart", json!({}));
        integration::ingest_inbox(&root, &mut casino).unwrap();
        let config = spool::load_config(&root).unwrap();
        let idle = casino.passive_sessions().unwrap()[0].clone();
        let lock = spool::lock_session(&root, "native-agent-session").unwrap();
        eligible(&root, &config.installation_id, &idle, &casino).unwrap();
        let journal = spool::load_journal(&root, "native-agent-session").unwrap();
        let wager = casino
            .arm_agent(
                ArmAgentRequest {
                    request_id: "a1a1a1a1-1111-4111-8111-111111111111".into(),
                    expected_wallet_revision: 0,
                    session_id: "native-agent-session".into(),
                    prediction: agent_core::Category::CleanRun,
                    stake: 100,
                },
                ArmAuthority {
                    installation_id: config.installation_id.clone(),
                    link_id: "native-test-link".into(),
                    watermark: journal.sequence,
                    required_suite_ids: policy(&root).unwrap(),
                    observed_at_ms: spool::now_ms(),
                },
            )
            .unwrap();
        drop(lock);
        enqueue(
            &root,
            "UserPromptSubmit",
            json!({"prompt":"private text never stored"}),
        );
        enqueue(
            &root,
            "PreToolUse",
            json!({"tool_use_id":"runner-tool","tool_name":"Bash","tool_input":{"command":"private source path"}}),
        );
        assert_eq!(
            verifier::verified_run(&root, &suite, "native-agent-session", "native-test-link")
                .unwrap(),
            0
        );
        enqueue(
            &root,
            "PostToolUse",
            json!({"tool_use_id":"runner-tool","tool_name":"Bash"}),
        );
        enqueue(&root, "Stop", json!({}));
        enqueue(&root, "SessionEnd", json!({}));
        assert!(integration::ingest_inbox(&root, &mut casino).unwrap());
        let settled = &casino.agent_wagers().unwrap()[0];
        assert_eq!(settled.outcome, Some(agent_core::Category::CleanRun));
        assert_eq!(settled.gross_return, 200);
        assert_eq!(casino.snapshot().unwrap()["wallet"]["balance"], 10100);
        assert!(casino.reconcile().unwrap());
        let database = temp.path().join("wallet.db");
        drop(casino);
        let mut recovered = Casino::open(&database).unwrap();
        assert!(!integration::ingest_inbox(&root, &mut recovered).unwrap());
        recovered
            .void_agent(
                "b2b2b2b2-2222-4222-8222-222222222222",
                &wager.wager_id,
                false,
            )
            .unwrap();
        assert_eq!(recovered.snapshot().unwrap()["wallet"]["balance"], 10100);
        assert!(recovered.reconcile().unwrap());
        drop(recovered);
        assert!(!String::from_utf8_lossy(&fs::read(database).unwrap())
            .contains("private name must never persist"));
    }
}
