use commit_casino_bridge::{installer, invalid, sanitizer, spool, verifier, BridgeResult};
use std::{
    env, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
fn root() -> BridgeResult<PathBuf> {
    let base = env::var_os("LOCALAPPDATA").ok_or_else(|| invalid("WINDOWS_APP_DATA_REQUIRED"))?;
    let p = PathBuf::from(base).join("CommitCasino").join("integration");
    installer::reject_reparse_chain(&p)?;
    Ok(p)
}
fn hook() -> BridgeResult<()> {
    // A hard watchdog covers stdin, locks, parsing and disk I/O; never block Claude.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(250));
        std::process::exit(0);
    });
    let root = root()?;
    // Persist BEFORE stdin can stall; watchdog/process death leaves the ticket.
    let ticket = spool::begin_hook(&root)?;
    let mut bytes = vec![];
    std::io::stdin()
        .take((sanitizer::MAX_INPUT + 1) as u64)
        .read_to_end(&mut bytes)?;
    let safe = sanitizer::sanitize(&bytes)?;
    let link = env::var("COMMIT_CASINO_LINK").ok();
    let event = spool::enqueue_hook(&root, safe, link.as_deref(), Some(&ticket))?;
    spool::complete_hook(&root, &ticket)?;
    // UI focus policy is native responsibility. Auto-launch is opt-in and prompt-only.
    let config = spool::load_config(&root)?;
    if config.auto_launch
        && event.observation.kind == sanitizer::HookType::TurnStart
        && !spool::load_journal(&root, &event.observation.session_id)?.evidence_gap
        && spool::should_auto_launch(&root, &event.observation.session_id).unwrap_or(false)
    {
        if let Some(executable) = config
            .desktop_executable
            .filter(|p| p.is_absolute() && p.is_file())
        {
            let mut cmd = Command::new(executable);
            cmd.arg("--passive-agent-prompt")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000 | 0x00000008);
            }
            let _ = cmd.spawn();
        }
    }
    Ok(())
}
fn launch_claude(root: &Path, executable: &Path, args: &[String]) -> BridgeResult<i32> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err(invalid("CONFIGURED_CLAUDE_EXE_REQUIRED"));
    }
    installer::reject_reparse_chain(executable)?;
    let config = spool::load_config(root)?;
    if config.provider_version != commit_casino_bridge::DOCUMENTED_FIXTURE_VERSION {
        return Err(invalid("VERSION_MISMATCH"));
    }
    verifier::verify_version(executable, &["--version".into()], &config.provider_version)?;
    let link = uuid::Uuid::new_v4().to_string();
    spool::atomic_write(
        &root.join("links").join(format!("{link}.json")),
        &serde_json::to_vec(&spool::LaunchLink {
            created_at_ms: spool::now_ms(),
            installation_id: config.installation_id,
            session_id: None,
        })?,
    )?;
    let status = Command::new(executable)
        .args(args)
        .env("COMMIT_CASINO_LINK", &link)
        .status()?;
    Ok(verifier::process_exit(status))
}
fn run(args: &[String]) -> BridgeResult<i32> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    if command == "hook" {
        if hook().is_err() {
            if let Ok(p) = root() {
                spool::note_gap(&p);
            }
        }
        return Ok(0);
    }
    let root = root()?;
    match command {
        "init" => {
            installer::initialize(
                &root,
                args.get(1).ok_or_else(|| invalid("VERSION_REQUIRED"))?,
            )?;
            Ok(0)
        }
        "preview" => {
            let helper = env::current_exe()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&installer::preview(&helper)?)?
            );
            Ok(0)
        }
        "register-claude" => {
            let path = PathBuf::from(args.get(1).ok_or_else(|| invalid("CLAUDE_EXE_REQUIRED"))?);
            if !path.is_absolute() || !path.is_file() {
                return Err(invalid("ABSOLUTE_CLAUDE_EXE_REQUIRED"));
            }
            installer::reject_reparse_chain(&path)?;
            verifier::verify_version(
                &path,
                &["--version".into()],
                commit_casino_bridge::DOCUMENTED_FIXTURE_VERSION,
            )?;
            spool::atomic_write(
                &root.join("claude-executable.json"),
                &serde_json::to_vec(&path)?,
            )?;
            Ok(0)
        }
        "register-suite" => {
            let path = PathBuf::from(
                args.get(1)
                    .ok_or_else(|| invalid("EXPLICIT_SUITE_CONFIG_REQUIRED"))?,
            );
            installer::reject_reparse_chain(&path)?;
            if fs::metadata(&path)?.len() > 8192 {
                return Err(invalid("SUITE_CONFIG_OVERSIZED"));
            }
            let suite: verifier::RegisteredSuite = serde_json::from_slice(&fs::read(path)?)?;
            verifier::validate_suite(&suite)?;
            if !sanitizer::opaque(&suite.suite_id)
                || !suite.executable.is_absolute()
                || !suite.working_directory.is_absolute()
            {
                return Err(invalid("INVALID_SUITE"));
            }
            verifier::verify_version(
                &suite.executable,
                &suite.version_args,
                &suite.runner_version,
            )?;
            spool::atomic_write(
                &root.join("suites").join(format!("{}.json", suite.suite_id)),
                &serde_json::to_vec_pretty(&suite)?,
            )?;
            Ok(0)
        }
        "require-suites" => {
            if args.len() < 2 || args.len() > 33 {
                return Err(invalid("REQUIRED_SUITE_IDS_REQUIRED"));
            }
            let mut seen = std::collections::BTreeSet::new();
            for id in &args[1..] {
                if !sanitizer::opaque(id) || !seen.insert(id) {
                    return Err(invalid("INVALID_REQUIRED_SUITE"));
                }
                let path = root.join("suites").join(format!("{id}.json"));
                spool::check_owned(&path)?;
                if fs::metadata(&path)?.len() > 8192 {
                    return Err(invalid("SUITE_CONFIG_OVERSIZED"));
                }
                let suite: verifier::RegisteredSuite = serde_json::from_slice(&fs::read(path)?)?;
                if &suite.suite_id != id {
                    return Err(invalid("SUITE_ID_MISMATCH"));
                }
                verifier::validate_suite(&suite)?;
                verifier::verify_version(
                    &suite.executable,
                    &suite.version_args,
                    &suite.runner_version,
                )?;
            }
            spool::atomic_write(
                &root.join("required-suites.json"),
                &serde_json::to_vec_pretty(&verifier::RequiredSuitePolicy {
                    suite_ids: args[1..].to_vec(),
                })?,
            )?;
            Ok(0)
        }
        "install" => {
            let settings = PathBuf::from(
                args.get(1)
                    .ok_or_else(|| invalid("SELECTED_SETTINGS_PATH_REQUIRED"))?,
            );
            let manifest = root.join("manifests").join(format!(
                "{}.json",
                spool::session_key(&settings.to_string_lossy())
            ));
            installer::install(&settings, &env::current_exe()?, &manifest)?;
            println!(
                "{}",
                serde_json::json!({"installed":true,"manifest":manifest})
            );
            Ok(0)
        }
        "disconnect" => {
            let path = PathBuf::from(args.get(1).ok_or_else(|| invalid("MANIFEST_REQUIRED"))?);
            if path.parent() != Some(root.join("manifests").as_path()) {
                return Err(invalid("OWNED_MANIFEST_REQUIRED"));
            }
            println!(
                "{}",
                serde_json::json!({"preservedMismatches":installer::disconnect(&path)?})
            );
            Ok(0)
        }
        "launch-claude" => {
            // Registration is explicit. Incoming hooks/UI never specify this executable.
            let executable: PathBuf =
                serde_json::from_slice(&fs::read(root.join("claude-executable.json"))?)?;
            launch_claude(&root, &executable, &args[1..])
        }
        "verify" => {
            let suite_id = args
                .get(1)
                .filter(|s| sanitizer::opaque(s))
                .ok_or_else(|| invalid("REGISTERED_SUITE_REQUIRED"))?;
            let path = root.join("suites").join(format!("{suite_id}.json"));
            spool::check_owned(&path)?;
            let suite: verifier::RegisteredSuite = serde_json::from_slice(&fs::read(path)?)?;
            if &suite.suite_id != suite_id {
                return Err(invalid("SUITE_ID_MISMATCH"));
            }
            let link =
                env::var("COMMIT_CASINO_LINK").map_err(|_| invalid("LAUNCH_LINK_REQUIRED"))?;
            let attribution = spool::read_link(&root, &link)?;
            let session = attribution
                .session_id
                .ok_or_else(|| invalid("SESSION_LINK_REQUIRED"))?;
            verifier::verified_run(&root, &suite, &session, &link)
        }
        _ => {
            println!("Commit Casino passive bridge: init VERSION | preview | install ABSOLUTE_SETTINGS_PATH | disconnect MANIFEST | launch-claude [args] | verify REGISTERED_SUITE SESSION");
            Ok(0)
        }
    }
}
fn main() {
    let args: Vec<_> = env::args().skip(1).collect();
    let is_hook = args.first().is_some_and(|s| s == "hook");
    match run(&args) {
        Ok(code) => std::process::exit(code),
        Err(_) => {
            if !is_hook {
                let _=std::io::stderr().write_all(b"Commit Casino bridge unavailable; configuration or evidence validation failed.\n");
            }
            std::process::exit(if is_hook { 0 } else { 1 });
        }
    }
}
