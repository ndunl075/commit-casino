//! Deterministic classifier. The native wallet authority supplies evidence derived
//! from authenticated stored observations; the frontend may never choose evidence.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const CLASSIFIER_VERSION: &str = "agent-classifier-v1";
pub const PAYOUT_VERSION: &str = "agent-v1";
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
pub const PENDING_TIMEOUT_MS: u64 = 900_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    #[serde(rename = "Clean Run")]
    CleanRun,
    Comeback,
    #[serde(rename = "Broken Build")]
    BrokenBuild,
    #[serde(rename = "Needs Help")]
    NeedsHelp,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckKind {
    Test,
    Build,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Integrity {
    Valid,
    Incomplete,
    Conflicting,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationRun {
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub suite_id: String,
    pub kind: CheckKind,
    pub started_sequence: u64,
    pub completed_sequence: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub observed_failed_attempts: u64,
    pub exit_code: i32,
    pub integrity: Integrity,
    pub supported: bool,
    pub canceled: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Boundary {
    NextTurn,
    SessionEnd,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TurnEvidence {
    pub session_id: String,
    pub turn_id: String,
    pub required_suite_ids: Vec<String>,
    pub required_build_ids: Vec<String>,
    pub runs: Vec<VerificationRun>,
    pub boundary: Option<Boundary>,
    pub boundary_sequence: u64,
    pub consumed_sequence: u64,
    pub latest_stop_sequence: Option<u64>,
    pub last_mutation_sequence: u64,
    pub continuous_coverage: bool,
    pub attribution_valid: bool,
    pub mutation_tracking_complete: bool,
    pub unaccounted_activity: bool,
    pub unresolved_attention: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub outcome: Category,
    pub reason: String,
    pub classifier_version: String,
    pub payout_version: String,
}
fn result(outcome: Category, reason: &str) -> Classification {
    Classification {
        outcome,
        reason: reason.into(),
        classifier_version: CLASSIFIER_VERSION.into(),
        payout_version: PAYOUT_VERSION.into(),
    }
}
fn unknown(reason: &str) -> Classification {
    result(Category::Unknown, reason)
}
fn safe(n: u64) -> bool {
    n <= MAX_SAFE_INTEGER
}
fn failed(r: &VerificationRun) -> bool {
    r.exit_code != 0 || r.failed > 0 || r.observed_failed_attempts > 0
}
pub fn classify_turn(e: &TurnEvidence) -> Classification {
    if e.boundary.is_none()
        || e.boundary_sequence == 0
        || !safe(e.boundary_sequence)
        || (matches!(e.boundary, Some(Boundary::NextTurn)) && e.latest_stop_sequence.is_none())
        || e.latest_stop_sequence
            .is_some_and(|s| !safe(s) || s >= e.boundary_sequence)
        || !safe(e.last_mutation_sequence)
        || e.last_mutation_sequence >= e.boundary_sequence
        || !safe(e.consumed_sequence)
        || e.consumed_sequence < e.boundary_sequence
    {
        return unknown("Terminal boundary or drained watermark unavailable");
    }
    if !e.continuous_coverage
        || !e.attribution_valid
        || !e.mutation_tracking_complete
        || e.unaccounted_activity
    {
        return unknown("Evidence coverage or attribution is uncertain");
    }
    let mut ids = BTreeSet::new();
    for r in &e.runs {
        if r.run_id.is_empty()
            || !ids.insert(&r.run_id)
            || r.session_id != e.session_id
            || r.turn_id != e.turn_id
            || r.started_sequence == 0
            || !safe(r.started_sequence)
            || r.completed_sequence == 0
            || !safe(r.completed_sequence)
            || r.completed_sequence <= r.started_sequence
            || r.completed_sequence >= e.boundary_sequence
            || r.integrity != Integrity::Valid
            || !r.supported
            || r.canceled
            || ![r.passed, r.failed, r.skipped, r.observed_failed_attempts]
                .iter()
                .all(|n| safe(*n))
            || r.exit_code < 0
            || (r.kind == CheckKind::Test && r.passed.saturating_add(r.failed) == 0)
            || (r.exit_code == 0 && (r.failed > 0 || r.observed_failed_attempts > 0))
            || (r.kind == CheckKind::Test
                && r.exit_code != 0
                && r.failed == 0
                && r.observed_failed_attempts == 0)
        {
            return unknown("Unsupported, incomplete, canceled, conflicting or unattributed run");
        }
    }
    let configured: Vec<_> = e
        .required_suite_ids
        .iter()
        .map(|s| (CheckKind::Test, s))
        .chain(e.required_build_ids.iter().map(|s| (CheckKind::Build, s)))
        .collect();
    let mut policy = BTreeSet::new();
    for (kind, id) in &configured {
        if id.is_empty() || !policy.insert((*kind, id.as_str())) {
            return unknown("Invalid verification policy");
        }
    }
    let mut observed: BTreeMap<(CheckKind, &str), Vec<&VerificationRun>> = BTreeMap::new();
    for r in &e.runs {
        observed.entry((r.kind, &r.suite_id)).or_default().push(r);
    }
    for runs in observed.values_mut() {
        runs.sort_by_key(|r| r.started_sequence);
        if runs
            .windows(2)
            .any(|pair| pair[0].completed_sequence >= pair[1].started_sequence)
        {
            return unknown("Overlapping runs of the same check are ambiguous");
        }
    }
    let finals: Vec<_> = configured
        .iter()
        .filter_map(|(kind, id)| {
            observed
                .get(&(*kind, id.as_str()))
                .and_then(|v| v.last())
                .copied()
        })
        .collect();
    if e.unresolved_attention {
        return result(
            Category::NeedsHelp,
            "User intervention unresolved at terminal boundary",
        );
    }
    if finals
        .iter()
        .any(|r| r.started_sequence <= e.last_mutation_sequence)
    {
        return unknown("Final verification predates possible workspace mutation");
    }
    if finals.iter().any(|r| failed(r)) {
        return result(Category::BrokenBuild, "Final required verification failed");
    }
    // Keep observed insertion order for explanation parity with the TS implementation.
    let mut builds = BTreeSet::new();
    for run in &e.runs {
        if run.kind == CheckKind::Build && builds.insert(run.suite_id.as_str()) {
            if observed
                .get(&(CheckKind::Build, run.suite_id.as_str()))
                .and_then(|v| v.last())
                .is_some_and(|r| r.exit_code != 0)
            {
                return result(Category::BrokenBuild, "Final registered build failed");
            }
        }
    }
    if e.required_suite_ids.is_empty() || finals.len() != configured.len() {
        return unknown("Every required test suite needs a fresh final pass");
    }
    if e.runs.iter().any(failed) {
        result(
            Category::Comeback,
            "Fresh required suites pass after observed failure",
        )
    } else {
        result(
            Category::CleanRun,
            "Fresh required suites pass without observed failures",
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settlement {
    pub gross_return: u64,
    pub net_change: i64,
    pub void: bool,
}
pub fn settle_prediction(
    prediction: Category,
    outcome: Category,
    stake: u64,
) -> Result<Settlement, &'static str> {
    if prediction == Category::Unknown
        || stake == 0
        || stake % 10 != 0
        || stake > MAX_SAFE_INTEGER / 2
    {
        return Err("INVALID_AGENT_STAKE");
    }
    let gross_return = if outcome == Category::Unknown {
        stake
    } else if prediction == outcome {
        stake * 2
    } else {
        0
    };
    Ok(Settlement {
        gross_return,
        net_change: gross_return as i64 - stake as i64,
        void: outcome == Category::Unknown,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_classification_contracts() {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/claude/agent-classifier-v1.json"
        ))
        .unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let evidence: TurnEvidence =
                serde_json::from_value(fixture["evidence"].clone()).unwrap();
            let actual = classify_turn(&evidence);
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                fixture["expected"],
                "{}",
                fixture["name"]
            );
        }
    }
    #[test]
    fn fixed_odds_and_refunds() {
        for prediction in [
            Category::CleanRun,
            Category::Comeback,
            Category::BrokenBuild,
            Category::NeedsHelp,
        ] {
            assert_eq!(
                settle_prediction(prediction, Category::Unknown, 100).unwrap(),
                Settlement {
                    gross_return: 100,
                    net_change: 0,
                    void: true
                }
            );
            assert_eq!(
                settle_prediction(prediction, prediction, 100)
                    .unwrap()
                    .gross_return,
                200
            );
        }
        assert!(settle_prediction(Category::Unknown, Category::Unknown, 10).is_err());
        assert!(
            settle_prediction(Category::CleanRun, Category::CleanRun, MAX_SAFE_INTEGER).is_err()
        );
    }
    #[test]
    fn strict_input_validation() {
        assert!(serde_json::from_str::<TurnEvidence>("{}").is_err());
    }
}
