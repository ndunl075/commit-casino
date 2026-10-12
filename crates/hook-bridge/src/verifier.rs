use crate::{
    invalid,
    spool::{atomic_write, check_owned, load_journal, lock_session, now_ms, private_dir},
    BridgeResult,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use uuid::Uuid;
pub fn verify_version(executable: &Path, args: &[String], expected: &str) -> BridgeResult<()> {
    let mut child = Command::new(executable)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(invalid("VERSION_PROBE_FAILED"));
            }
            let mut bytes = vec![];
            use std::io::Read;
            if let Some(out) = child.stdout.take() {
                out.take(4096).read_to_end(&mut bytes)?;
            }
            let text = String::from_utf8(bytes)?;
            if text
                .split(|c: char| !(c.is_ascii_digit() || c == '.'))
                .any(|token| token == expected)
            {
                return Ok(());
            }
            return Err(invalid("VERSION_MISMATCH"));
        }
        if start.elapsed() > std::time::Duration::from_secs(2) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(invalid("VERSION_PROBE_TIMEOUT"));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisteredSuite {
    pub suite_id: String,
    pub runner: String,
    pub runner_version: String,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub version_args: Vec<String>,
    pub working_directory: PathBuf,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunSummary {
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub suite_id: String,
    pub runner: String,
    pub runner_version: String,
    pub exit_code: i32,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub observed_failed_attempts: u64,
    pub started_sequence: u64,
    pub completed_sequence: u64,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub integrity: String,
}
pub const VITEST_FIXTURE_VERSION: &str = "5.0.3";
pub const PLAYWRIGHT_FIXTURE_VERSION: &str = "1.64.0";
fn nonnegative(v: &Value) -> BridgeResult<u64> {
    v.as_u64()
        .filter(|n| *n <= 9_007_199_254_740_991)
        .ok_or_else(|| invalid("INVALID_REPORT_COUNT"))
}
pub fn normalize_report(
    runner: &str,
    version: &str,
    report: &Value,
    exit: i32,
) -> BridgeResult<(u64, u64, u64, u64)> {
    let (passed, failed, skipped, attempts) = match (runner, version) {
        ("vitest", VITEST_FIXTURE_VERSION) => {
            let p = nonnegative(&report["numPassedTests"])?;
            let f = nonnegative(&report["numFailedTests"])?;
            let s =
                nonnegative(&report["numPendingTests"])? + nonnegative(&report["numTodoTests"])?;
            if p.checked_add(f).and_then(|n| n.checked_add(s))
                != Some(nonnegative(&report["numTotalTests"])?)
                || report["success"].as_bool() != Some(exit == 0)
            {
                return Err(invalid("CONFLICTING_REPORT"));
            }
            // This Vitest JSON schema does not expose every retry attempt reliably.
            // Suites must explicitly disable retries; wrapper registration rejects retry flags.
            (p, f, s, f)
        }
        ("playwright", PLAYWRIGHT_FIXTURE_VERSION) => {
            let p = nonnegative(&report["stats"]["expected"])?;
            let f = nonnegative(&report["stats"]["unexpected"])?;
            let s = nonnegative(&report["stats"]["skipped"])?;
            let flaky = nonnegative(&report["stats"]["flaky"])?;
            if flaky > 0 {
                return Err(invalid("FLAKY_FINAL_REPORT"));
            }
            if !report["errors"].as_array().is_some_and(|a| a.is_empty()) {
                return Err(invalid("REPORT_ERRORS"));
            }
            (p, f, s, f + flaky)
        }
        _ => return Err(invalid("UNSUPPORTED_RUNNER_VERSION")),
    };
    if passed + failed == 0 || (exit == 0) != (failed == 0) || exit < 0 {
        return Err(invalid("ZERO_TESTS_OR_EXIT_CONFLICT"));
    }
    Ok((passed, failed, skipped, attempts))
}
pub fn process_exit(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        128 + status.signal().unwrap_or(1)
    }
    #[cfg(not(unix))]
    {
        1
    }
}
/** Only explicit CLI calls load a registered command; hook payloads cannot supply commands.
 * This preserves inherited stdout/stderr and the actual child exit even when evidence fails. */
pub fn verified_run(
    root: &Path,
    suite: &RegisteredSuite,
    session: &str,
    link: &str,
) -> BridgeResult<i32> {
    if !crate::sanitizer::opaque(&suite.suite_id)
        || !crate::sanitizer::opaque(link)
        || !suite.executable.is_absolute()
        || !suite.working_directory.is_absolute()
    {
        return Err(invalid("INVALID_SUITE_REGISTRATION"));
    }
    if !matches!(
        (suite.runner.as_str(), suite.runner_version.as_str()),
        ("vitest", VITEST_FIXTURE_VERSION) | ("playwright", PLAYWRIGHT_FIXTURE_VERSION)
    ) {
        return Err(invalid("UNSUPPORTED_RUNNER_VERSION"));
    }
    if suite.args.iter().any(|s| {
        s.contains("retry")
            || s.contains("retries")
            || s.contains("outputFile")
            || s.contains("reporter")
    }) {
        return Err(invalid("RUNNER_FLAGS_OWNED_BY_WRAPPER"));
    }
    crate::installer::reject_reparse_chain(&suite.executable)?;
    crate::installer::reject_reparse_chain(&suite.working_directory)?;
    verify_version(
        &suite.executable,
        &suite.version_args,
        &suite.runner_version,
    )?;
    let run_id = Uuid::new_v4().to_string();
    let run_dir = root.join("runs").join(&run_id);
    private_dir(&run_dir)?;
    let report_path = run_dir.join("report.json");
    let started_at_ms = now_ms();
    let mut summary = {
        let _lock = lock_session(root, session)?;
        let mut journal = load_journal(root, session)?;
        if journal.evidence_gap
            || journal.ended
            || journal.stopped
            || journal.link_id.as_deref() != Some(link)
            || journal.turn_id.is_none()
        {
            return Err(invalid("AMBIGUOUS_RUN_ATTRIBUTION"));
        }
        let turn = journal.turn_id.clone().ok_or_else(|| invalid("NO_TURN"))?;
        let mut summary = RunSummary {
            run_id: run_id.clone(),
            session_id: session.into(),
            turn_id: turn,
            suite_id: suite.suite_id.clone(),
            runner: suite.runner.clone(),
            runner_version: suite.runner_version.clone(),
            exit_code: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            observed_failed_attempts: 0,
            started_sequence: 0,
            completed_sequence: 0,
            started_at_ms,
            completed_at_ms: 0,
            integrity: "incomplete".into(),
        };
        crate::spool::publish_verification(root, session, &mut journal, &mut summary, true)?;
        atomic_write(
            &run_dir.join("started.json"),
            &serde_json::to_vec(&summary)?,
        )?;
        summary
    };
    let mut command = Command::new(&suite.executable);
    command
        .args(&suite.args)
        .current_dir(&suite.working_directory)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if suite.runner == "vitest" {
        command
            .arg("--reporter=json")
            .arg(format!("--outputFile={}", report_path.display()))
            .arg("--retry=0");
    } else {
        command
            .args(["--reporter=json", "--retries=0"])
            .env("PLAYWRIGHT_JSON_OUTPUT_FILE", &report_path);
    }
    let exit = process_exit(command.status()?);
    // Never replace the child exit code with a report/attribution error.
    let record = (|| -> BridgeResult<()> {
        let _lock = lock_session(root, session)?;
        let mut journal = load_journal(root, session)?;
        summary.exit_code = exit;
        summary.completed_at_ms = now_ms();
        if journal.turn_id.as_deref() == Some(&summary.turn_id)
            && !journal.evidence_gap
            && !journal.ended
            && journal.link_id.as_deref() == Some(link)
            && report_path.exists()
        {
            check_owned(&report_path)?;
            let metadata = fs::metadata(&report_path)?;
            if metadata.len() <= 10 * 1024 * 1024
                && metadata
                    .modified()?
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis()
                    >= started_at_ms as u128
            {
                let report: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
                if let Ok((p, f, s, a)) =
                    normalize_report(&suite.runner, &suite.runner_version, &report, exit)
                {
                    summary.passed = p;
                    summary.failed = f;
                    summary.skipped = s;
                    summary.observed_failed_attempts = a;
                    summary.integrity = "valid".into();
                }
            }
        }
        crate::spool::publish_verification(root, session, &mut journal, &mut summary, false)?;
        atomic_write(
            &run_dir.join("summary.json"),
            &serde_json::to_vec(&summary)?,
        )?;
        Ok(())
    })();
    // Raw runner names/paths are never retained as diagnostics after normalization.
    if report_path.exists() {
        let _ = fs::remove_file(&report_path);
    }
    if record.is_err() {
        crate::spool::note_gap(root);
        let _ = atomic_write(&run_dir.join("incomplete"), b"EVIDENCE_INCOMPLETE");
    }
    Ok(exit)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vitest_counts_conflicts_and_zero_tests() {
        let v = serde_json::json!({"numPassedTests":2,"numFailedTests":0,"numPendingTests":1,"numTodoTests":0,"numTotalTests":3,"success":true});
        assert_eq!(
            normalize_report("vitest", VITEST_FIXTURE_VERSION, &v, 0).unwrap(),
            (2, 0, 1, 0)
        );
        assert!(normalize_report("vitest", VITEST_FIXTURE_VERSION, &v, 1).is_err());
        assert!(normalize_report("vitest", "unknown", &v, 0).is_err());
        assert!(normalize_report("vitest",VITEST_FIXTURE_VERSION,&serde_json::json!({"numPassedTests":0,"numFailedTests":0,"numPendingTests":0,"numTodoTests":0,"numTotalTests":0,"success":true}),0).is_err());
    }
    #[test]
    fn playwright_flaky_pass_is_not_clean() {
        let v = serde_json::json!({"stats":{"expected":1,"unexpected":0,"skipped":0,"flaky":1},"errors":[]});
        assert!(normalize_report("playwright", PLAYWRIGHT_FIXTURE_VERSION, &v, 0).is_err());
    }
}
