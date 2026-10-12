use super::*;
#[test]
fn passive_lifecycle_is_contiguous_and_financially_isolated() {
    let mut c = game();
    let clock = Utc::now().timestamp_millis() as u64;
    let event = |sequence, event_type| PassiveObservation {
        installation: "install",
        session: "session",
        event_id: if sequence == 1 {
            "event-one"
        } else {
            "event-two"
        },
        sequence,
        event_type,
        turn_id: Some("turn"),
        observed_at_ms: clock,
    };
    assert!(c.ingest_passive(event(2, "turn-start")).is_err());
    assert!(c.ingest_passive(event(1, "session-start")).unwrap());
    assert!(!c.ingest_passive(event(1, "session-start")).unwrap());
    assert!(c.ingest_passive(event(2, "attention-requested")).unwrap());
    assert_eq!(c.consumed_sequence("install", "session").unwrap(), 2);
    assert_eq!(c.passive_sessions().unwrap()[0]["status"], "attention");
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10000);
    assert_eq!(c.snapshot().unwrap()["features"]["agentRoulette"], false);
    assert!(c.reconcile().unwrap());
}
#[test]
fn failed_settlement_rolls_back_credit_and_recovers() {
    let mut c = game();
    let round = c.commit_with(request(0), || Ok(7)).unwrap();
    c.db.execute_batch("CREATE TRIGGER failed_credit BEFORE INSERT ON wallet_ledger WHEN NEW.kind='return' BEGIN SELECT RAISE(ABORT,'fail'); END;").unwrap();
    assert!(c.settle(&round.round_id).is_err());
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 9850);
    c.db.execute_batch("DROP TRIGGER failed_credit;").unwrap();
    c.settle(&round.round_id).unwrap();
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 13550);
    assert!(c.reconcile().unwrap());
}
fn request(revision: i64) -> CommitRequest {
    CommitRequest {
        request_id: Uuid::new_v4().to_string(),
        expected_wallet_revision: revision,
        rules_version: "european-v1".into(),
        bets: vec![
            BetInput {
                bet_id: "straight:7".into(),
                stake: 100,
            },
            BetInput {
                bet_id: "outside:red".into(),
                stake: 50,
            },
        ],
        animation_mode: "fast".into(),
    }
}
fn game() -> Casino {
    Casino::open(":memory:").unwrap()
}
#[test]
fn rejection_boundary() {
    let mut v = vec![u32::MAX, 4_294_967_288, 36].into_iter();
    assert_eq!(uniform_pocket(|| Ok(v.next().unwrap())).unwrap(), 36);
    assert_eq!(
        uniform_pocket(|| Err(Error::new("RNG_UNAVAILABLE", "failure")))
            .unwrap_err()
            .code,
        "RNG_UNAVAILABLE"
    )
}
#[test]
fn atomic_settlement_and_retry() {
    let mut c = game();
    let req = request(0);
    let round = c.commit_with(req.clone(), || Ok(7)).unwrap();
    assert_eq!(round.gross_return, 3700);
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 9850);
    let retry = c
        .commit_with(req.clone(), || panic!("retry must not reroll"))
        .unwrap();
    assert_eq!(retry.outcome, 7);
    let mut changed = req;
    changed.bets[0].stake = 200;
    assert_eq!(c.commit(changed).unwrap_err().code, "INVALID_BET");
    c.settle(&round.round_id).unwrap();
    c.settle(&round.round_id).unwrap();
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 13550);
    assert_eq!(c.snapshot().unwrap()["stats"]["spins"], 1);
    assert!(c.reconcile().unwrap());
    c.acknowledge(&round.round_id).unwrap();
    assert!(c.snapshot().unwrap()["activeRound"].is_null())
}
#[test]
fn entropy_failure_has_no_debit() {
    let mut c = game();
    assert_eq!(
        c.commit_with(request(0), || Err(Error::new("RNG_UNAVAILABLE", "failure")))
            .unwrap_err()
            .code,
        "RNG_UNAVAILABLE"
    );
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10000);
    assert!(c.reconcile().unwrap())
}
#[test]
fn invalid_stake_revision_and_active() {
    let mut c = game();
    let mut r = request(0);
    r.bets[0].stake = 11;
    assert_eq!(c.commit(r).unwrap_err().code, "INVALID_BET");
    assert_eq!(c.commit(request(9)).unwrap_err().code, "STALE_REVISION");
    let round = c.commit_with(request(0), || Ok(0)).unwrap();
    assert_eq!(c.commit(request(1)).unwrap_err().code, "ROUND_ACTIVE");
    assert_eq!(
        c.wallet_action(&Uuid::new_v4().to_string(), true)
            .unwrap_err()
            .code,
        "ROUND_ACTIVE"
    );
    c.settle(&round.round_id).unwrap();
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 9850)
}
#[test]
fn crash_recovers_recorded_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wallet.db");
    let id;
    {
        let mut c = Casino::open(&path).unwrap();
        id = c.commit_with(request(0), || Ok(7)).unwrap().round_id;
    }
    let mut c = Casino::open(&path).unwrap();
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 13550);
    assert_eq!(c.snapshot().unwrap()["activeRound"]["outcome"], 7);
    c.settle(&id).unwrap();
    assert!(c.reconcile().unwrap());
    c.acknowledge(&id).unwrap();
    drop(c);
    assert!(Casino::open(&path).unwrap().snapshot().unwrap()["activeRound"].is_null())
}
#[test]
fn all_catalog_bets_and_outcomes() {
    let mut c = game();
    let catalog: Catalog = serde_json::from_str(CATALOG).unwrap();
    for entry in catalog.bets {
        for outcome in 0..37 {
            let mut req = request(
                c.db.query_row("SELECT revision FROM wallet WHERE id=1", [], |r| r.get(0))
                    .unwrap(),
            );
            req.bets = vec![BetInput {
                bet_id: entry.id.clone(),
                stake: 10,
            }];
            let round = c.commit_with(req, || Ok(outcome)).unwrap();
            assert_eq!(
                round.gross_return,
                if entry.numbers.contains(&outcome) {
                    10 * (entry.profit_odds + 1)
                } else {
                    0
                }
            );
            c.settle(&round.round_id).unwrap();
            c.acknowledge(&round.round_id).unwrap();
            if c.db
                .query_row("SELECT balance FROM wallet WHERE id=1", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
                < 10
            {
                c.wallet_action(&Uuid::new_v4().to_string(), true).unwrap();
            }
        }
    }
    assert!(c.reconcile().unwrap())
}
#[test]
fn reset_and_clear_preserve_replay() {
    let mut c = game();
    let req = request(0);
    let id = req.request_id.clone();
    let r = c.commit_with(req.clone(), || Ok(7)).unwrap();
    c.settle(&r.round_id).unwrap();
    c.acknowledge(&r.round_id).unwrap();
    let reset_id = Uuid::new_v4().to_string();
    let reset = c.wallet_action(&reset_id, true).unwrap();
    assert_eq!(c.wallet_action(&reset_id, true).unwrap(), reset);
    let clear_id = Uuid::new_v4().to_string();
    let clear = c.clear_history(&clear_id).unwrap();
    assert_eq!(c.clear_history(&clear_id).unwrap(), clear);
    assert_eq!(c.commit(req).unwrap().request_id, id);
    assert!(c.reconcile().unwrap());
    assert_eq!(c.snapshot().unwrap()["history"], json!([]))
}
#[test]
fn storage_failure_rolls_back() {
    let mut c = game();
    c.db.execute_batch("CREATE TRIGGER injected BEFORE INSERT ON wallet_ledger WHEN NEW.kind='stake' BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert_eq!(
        c.commit_with(request(0), || Ok(7)).unwrap_err().code,
        "STORAGE_UNAVAILABLE"
    );
    assert_eq!(c.snapshot().unwrap()["wallet"]["balance"], 10000);
    assert!(c.snapshot().unwrap()["history"]
        .as_array()
        .unwrap()
        .is_empty())
}
#[test]
fn settings_are_validated() {
    let mut c = game();
    assert!(c.update_settings(json!({"volume":2})).is_err());
    assert!(c.update_settings(json!({"arbitrary":true})).is_err());
    assert_eq!(
        c.update_settings(json!({"mute":true,"windowMode":"compact"}))
            .unwrap()["settings"]["mute"],
        true
    )
}
