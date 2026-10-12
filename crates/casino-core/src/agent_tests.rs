use super::*;
use crate::agent::*;
use agent_core::{Category, CheckKind, Integrity, VerificationRun};
#[test]
fn needs_help_waits_for_boundary_and_explicit_void_refunds() {
    let mut c = game();
    start(&mut c);
    arm(&mut c, Category::NeedsHelp);
    event(&mut c, 2, "turn-start", "turn", meta());
    event(&mut c, 3, "attention-requested", "turn", meta());
    assert_eq!(c.agent_wagers().unwrap()[0].status, "bound");
    event(&mut c, 4, "session-end", "turn", meta());
    assert_eq!(
        c.agent_wagers().unwrap()[0].outcome,
        Some(Category::NeedsHelp)
    );
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10100);
    let mut c = game();
    start(&mut c);
    let wager = arm(&mut c, Category::CleanRun);
    event(&mut c, 2, "turn-start", "turn", meta());
    assert_eq!(
        c.void_agent(&Uuid::new_v4().to_string(), &wager.wager_id, true)
            .unwrap_err()
            .code,
        "ROUND_ACTIVE"
    );
    c.void_agent(&Uuid::new_v4().to_string(), &wager.wager_id, false)
        .unwrap();
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10000);
    assert!(c.reconcile().unwrap());
}
fn game() -> Casino {
    Casino::open(":memory:").unwrap()
}
fn arm(c: &mut Casino, prediction: Category) -> AgentWager {
    let request = ArmAgentRequest {
        request_id: Uuid::new_v4().to_string(),
        expected_wallet_revision: c.snapshot().unwrap()["wallet"]["revision"]
            .as_i64()
            .unwrap(),
        session_id: "session".into(),
        prediction,
        stake: 100,
    };
    c.arm_agent(
        request,
        ArmAuthority {
            installation_id: "installation".into(),
            link_id: "link".into(),
            watermark: 1,
            required_suite_ids: vec!["tests".into()],
            observed_at_ms: 1000,
        },
    )
    .unwrap()
}
fn run(failed: bool) -> VerificationRun {
    VerificationRun {
        run_id: "run".into(),
        session_id: "session".into(),
        turn_id: "turn".into(),
        suite_id: "tests".into(),
        kind: CheckKind::Test,
        started_sequence: 4,
        completed_sequence: if failed { 5 } else { 0 },
        passed: if failed { 0 } else { 1 },
        failed: if failed { 1 } else { 0 },
        skipped: 0,
        observed_failed_attempts: 0,
        exit_code: if failed { 1 } else { 0 },
        integrity: if failed {
            Integrity::Valid
        } else {
            Integrity::Incomplete
        },
        supported: true,
        canceled: false,
    }
}
fn event(c: &mut Casino, sequence: u64, kind: &str, turn: &str, mut metadata: AgentObservation) {
    if kind == "verified-run-completed" {
        let run = metadata.verification.as_mut().unwrap();
        run.completed_sequence = sequence;
        if run.failed == 0 {
            run.integrity = Integrity::Valid;
        }
    }
    let event_id = format!("event-{sequence}");
    c.ingest_passive(PassiveObservation {
        installation: "installation",
        session: "session",
        event_id: &event_id,
        sequence,
        event_type: kind,
        turn_id: Some(turn),
        observed_at_ms: 1000 + sequence,
        agent: Some(metadata),
    })
    .unwrap();
}
fn meta() -> AgentObservation {
    AgentObservation {
        tool_id: Some("tool".into()),
        effect: Some("possible-mutation".into()),
        verification: None,
        journal_gap: false,
        link_id: Some("link".into()),
    }
}
fn start(c: &mut Casino) {
    event(c, 1, "session-start", "previous", meta());
}
fn pass(c: &mut Casino, failed: bool) {
    event(c, 2, "turn-start", "turn", meta());
    event(c, 3, "tool-start", "turn", meta());
    let mut m = meta();
    let mut started = run(false);
    started.passed = 0;
    m.verification = Some(started);
    event(c, 4, "verified-run-started", "turn", m);
    let mut m = meta();
    m.verification = Some(run(failed));
    event(c, 5, "verified-run-completed", "turn", m);
    event(c, 6, "tool-end", "turn", meta());
    event(c, 7, "stop-candidate", "turn", meta());
}
#[test]
fn verified_win_settles_once_at_boundary() {
    let mut c = game();
    start(&mut c);
    let wager = arm(&mut c, Category::CleanRun);
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 9900);
    pass(&mut c, false);
    assert_eq!(c.agent_wagers().unwrap()[0].status, "bound");
    event(&mut c, 8, "turn-start", "next", meta());
    let result = &c.agent_wagers().unwrap()[0];
    assert_eq!(result.outcome, Some(Category::CleanRun));
    assert_eq!(result.gross_return, 200);
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10100);
    c.void_agent(&Uuid::new_v4().to_string(), &wager.wager_id, false)
        .unwrap();
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10100);
    assert!(c.reconcile().unwrap());
}
#[test]
fn incorrect_prediction_loses_and_unknown_refunds() {
    let mut c = game();
    start(&mut c);
    arm(&mut c, Category::CleanRun);
    pass(&mut c, true);
    event(&mut c, 8, "session-end", "turn", meta());
    assert_eq!(
        c.agent_wagers().unwrap()[0].outcome,
        Some(Category::BrokenBuild)
    );
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 9900);
    let mut c = game();
    start(&mut c);
    arm(&mut c, Category::CleanRun);
    event(&mut c, 2, "turn-start", "turn", meta());
    event(&mut c, 3, "session-end", "turn", meta());
    assert_eq!(
        c.agent_wagers().unwrap()[0].outcome,
        Some(Category::Unknown)
    );
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10000);
    assert_eq!(c.snapshot().unwrap()["agentStats"]["voids"], 1);
}
#[test]
fn timeout_cancel_replay_and_wallet_reset_guards() {
    let mut c = game();
    start(&mut c);
    let wager = arm(&mut c, Category::NeedsHelp);
    assert_eq!(
        c.wallet_action(&Uuid::new_v4().to_string(), true)
            .unwrap_err()
            .code,
        "ROUND_ACTIVE"
    );
    let request_id = Uuid::new_v4().to_string();
    c.void_agent(&request_id, &wager.wager_id, true).unwrap();
    c.void_agent(&request_id, &wager.wager_id, true).unwrap();
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10000);
    let mut c = game();
    start(&mut c);
    arm(&mut c, Category::NeedsHelp);
    assert!(!c.expire_agent(1000 + 899999).unwrap());
    assert!(c.expire_agent(1000 + 900000).unwrap());
    assert!(!c.expire_agent(2000000).unwrap());
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10000);
    assert!(c.reconcile().unwrap());
}
#[test]
fn gaps_and_mutation_after_test_fail_closed() {
    let mut c = game();
    start(&mut c);
    arm(&mut c, Category::CleanRun);
    pass(&mut c, false);
    let mut m = meta();
    m.journal_gap = true;
    event(&mut c, 8, "session-end", "turn", m);
    assert_eq!(
        c.agent_wagers().unwrap()[0].outcome,
        Some(Category::Unknown)
    );
    let mut c = game();
    start(&mut c);
    arm(&mut c, Category::CleanRun);
    pass(&mut c, false);
    event(&mut c, 8, "tool-start", "turn", meta());
    event(&mut c, 9, "tool-end", "turn", meta());
    event(&mut c, 10, "stop-candidate", "turn", meta());
    event(&mut c, 11, "session-end", "turn", meta());
    assert_eq!(
        c.agent_wagers().unwrap()[0].outcome,
        Some(Category::Unknown)
    );
    assert!(c.reconcile().unwrap());
}

#[test]
fn failed_agent_credit_rolls_back_event_watermark_and_recovers() {
    let mut c = game();
    start(&mut c);
    arm(&mut c, Category::CleanRun);
    pass(&mut c, false);
    c.db.execute_batch("CREATE TRIGGER agent_credit_failure BEFORE INSERT ON wallet_ledger WHEN NEW.kind='agent-return' BEGIN SELECT RAISE(ABORT,'fail'); END;").unwrap();
    let observation = || PassiveObservation {
        installation: "installation",
        session: "session",
        event_id: "boundary",
        sequence: 8,
        event_type: "session-end",
        turn_id: Some("turn"),
        observed_at_ms: 1008,
        agent: Some(meta()),
    };
    assert!(c.ingest_passive(observation()).is_err());
    assert_eq!(c.consumed_sequence("installation", "session").unwrap(), 7);
    assert_eq!(c.agent_wagers().unwrap()[0].status, "bound");
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 9900);
    c.db.execute_batch("DROP TRIGGER agent_credit_failure;")
        .unwrap();
    c.ingest_passive(observation()).unwrap();
    c.ingest_passive(observation()).unwrap();
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10100);
    assert!(c.reconcile().unwrap());
}
