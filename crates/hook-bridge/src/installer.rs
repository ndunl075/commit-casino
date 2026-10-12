use crate::{
    invalid,
    spool::{atomic_write, check_owned, private_dir, Configuration},
    BridgeResult, DOCUMENTED_FIXTURE_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;
pub const EVENTS: [&str; 9] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "SessionEnd",
];
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CliDetection {
    pub available: bool,
    pub supported: bool,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub reason: String,
}
/** Resolve only the fixed provider executable. Never accept arbitrary UI commands. */
pub fn detect_supported_cli() -> CliDetection {
    let candidate = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| {
                dir.join(if cfg!(windows) {
                    "claude.exe"
                } else {
                    "claude"
                })
            })
            .find(|path| path.is_absolute() && path.is_file())
    });
    let Some(path) = candidate else {
        return CliDetection {
            available: false,
            supported: false,
            executable: None,
            version: None,
            reason: "CLAUDE_CLI_UNAVAILABLE".into(),
        };
    };
    if reject_reparse_chain(&path).is_err() {
        return CliDetection {
            available: true,
            supported: false,
            executable: Some(path),
            version: None,
            reason: "CLAUDE_PATH_UNSAFE".into(),
        };
    }
    let supported =
        crate::verifier::verify_version(&path, &["--version".into()], DOCUMENTED_FIXTURE_VERSION)
            .is_ok();
    CliDetection {
        available: true,
        supported,
        executable: Some(path),
        version: if supported {
            Some(DOCUMENTED_FIXTURE_VERSION.into())
        } else {
            None
        },
        reason: if supported {
            "DOCUMENTED_PASSIVE_PROFILE; LIVE_AGENT_GATE_PENDING"
        } else {
            "VERSION_MISMATCH_OR_PROBE_FAILED"
        }
        .into(),
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallManifest {
    pub version: u8,
    pub settings_path: PathBuf,
    pub original_hash: String,
    pub entries: Vec<(String, Value)>,
}
fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
pub fn preview(helper: &Path) -> BridgeResult<Vec<(String, Value)>> {
    if !helper.is_absolute() || helper.extension().and_then(|s| s.to_str()) != Some("exe") {
        return Err(invalid("ABSOLUTE_EXE_REQUIRED"));
    }
    Ok(EVENTS.iter().map(|event| ((*event).into(), json!({"matcher":"*","hooks":[{"type":"command","command":helper,"args":["hook"],"timeout":1}]}))).collect())
}
fn read_settings(path: &Path) -> BridgeResult<(Vec<u8>, Value)> {
    if !path.is_absolute() {
        return Err(invalid("ABSOLUTE_SETTINGS_PATH_REQUIRED"));
    }
    if path.exists() {
        reject_reparse_chain(path)?;
        if fs::metadata(path)?.len() > 1024 * 1024 {
            return Err(invalid("SETTINGS_OVERSIZED"));
        }
        let bytes = fs::read(path)?;
        let parsed: Value = serde_json::from_slice(&bytes)?;
        if !parsed.is_object() || parsed.get("hooks").is_some_and(|v| !v.is_object()) {
            return Err(invalid("MALFORMED_SETTINGS"));
        }
        if parsed["disableAllHooks"] == true || parsed["allowManagedHooksOnly"] == true {
            return Err(invalid("HOOKS_RESTRICTED"));
        }
        Ok((bytes, parsed))
    } else {
        Ok((vec![], json!({})))
    }
}
pub fn reject_reparse_chain(path: &Path) -> BridgeResult<()> {
    for ancestor in path.ancestors() {
        if ancestor.exists() {
            let m = fs::symlink_metadata(ancestor)?;
            if m.file_type().is_symlink() {
                return Err(invalid("SYMLINK_REJECTED"));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if m.file_attributes() & 0x400 != 0 {
                    return Err(invalid("REPARSE_POINT_REJECTED"));
                }
            }
        }
    }
    Ok(())
}
pub fn initialize(root: &Path, provider_version: &str) -> BridgeResult<()> {
    if !root.is_absolute() || root.exists() || provider_version != DOCUMENTED_FIXTURE_VERSION {
        return Err(invalid(
            "FRESH_PRIVATE_ROOT_AND_DOCUMENTED_VERSION_REQUIRED",
        ));
    }
    reject_reparse_chain(root)?;
    private_dir(root)?;
    #[cfg(windows)]
    {
        // Setup only; never spawn this command from a hook. Restrict the new tree to current SID.
        let output = std::process::Command::new("whoami.exe")
            .args(["/user", "/fo", "csv", "/nh"])
            .output()?;
        if !output.status.success() {
            return Err(invalid("IDENTITY_UNAVAILABLE"));
        }
        let text = String::from_utf8(output.stdout)?;
        let sid = text
            .split(',')
            .nth(1)
            .map(|s| s.trim().trim_matches('"'))
            .filter(|s| s.starts_with("S-1-"))
            .ok_or_else(|| invalid("IDENTITY_UNAVAILABLE"))?;
        let status = std::process::Command::new("icacls.exe")
            .arg(root)
            .args(["/inheritance:r", "/grant:r"])
            .arg(format!("*{sid}:(OI)(CI)F"))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        if !status.success() {
            return Err(invalid("PRIVATE_ACL_FAILED"));
        }
    }
    for name in ["inbox", "journals", "runs", "links", "manifests", "suites"] {
        private_dir(&root.join(name))?;
    }
    let mut secret = [0u8; 32];
    getrandom::fill(&mut secret).map_err(|_| invalid("SECURE_ENTROPY_UNAVAILABLE"))?;
    let config = Configuration {
        version: 1,
        installation_id: Uuid::new_v4().to_string(),
        secret: hex::encode(secret),
        provider_version: provider_version.into(),
        auto_launch: false,
        desktop_executable: None,
    };
    atomic_write(
        &root.join("config.json"),
        &serde_json::to_vec_pretty(&config)?,
    )
}
/** Explicit caller action after preview. Preserve every unrelated setting and hook. */
pub fn install(
    settings: &Path,
    helper: &Path,
    manifest_path: &Path,
) -> BridgeResult<InstallManifest> {
    if manifest_path.exists() {
        return Err(invalid("ALREADY_INSTALLED"));
    }
    if !helper.is_file() {
        return Err(invalid("HELPER_MISSING"));
    }
    let entries = preview(helper)?;
    let (original, mut document) = read_settings(settings)?;
    let object = document
        .as_object_mut()
        .ok_or_else(|| invalid("MALFORMED_SETTINGS"))?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| invalid("MALFORMED_HOOKS"))?;
    for (name, entry) in &entries {
        let list = hooks
            .entry(name)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| invalid("MALFORMED_HOOK_ENTRIES"))?;
        if list.contains(entry) {
            return Err(invalid("UNOWNED_MATCHING_ENTRY"));
        }
        list.push(entry.clone());
    }
    let manifest = InstallManifest {
        version: 1,
        settings_path: settings.into(),
        original_hash: hash(&original),
        entries,
    };
    let parent = settings.parent().ok_or_else(|| invalid("NO_PARENT"))?;
    reject_reparse_chain(parent)?;
    fs::create_dir_all(parent)?;
    // Settings aren't an integration-private location; atomic_write checks Unix private permissions.
    if !original.is_empty() {
        atomic_write(&manifest_path.with_extension("backup.json"), &original)?;
    }
    atomic_write(manifest_path, &serde_json::to_vec_pretty(&manifest)?)?;
    write_settings(settings, &serde_json::to_vec_pretty(&document)?)?;
    Ok(manifest)
}
fn write_settings(path: &Path, bytes: &[u8]) -> BridgeResult<()> {
    // Scope to selected settings path, without relaxing the private spool policy.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let parent = path.parent().ok_or_else(|| invalid("NO_PARENT"))?;
        let prior = fs::metadata(parent)?.permissions();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        let result = atomic_write(path, bytes);
        fs::set_permissions(parent, prior)?;
        return result;
    }
    #[cfg(not(unix))]
    atomic_write(path, bytes)
}
pub fn disconnect(manifest_path: &Path) -> BridgeResult<Vec<String>> {
    check_owned(manifest_path)?;
    let manifest: InstallManifest = serde_json::from_slice(&fs::read(manifest_path)?)?;
    if manifest.version != 1 {
        return Err(invalid("VERSION_MISMATCH"));
    }
    let (_, mut document) = read_settings(&manifest.settings_path)?;
    let mut mismatches = vec![];
    for (name, owned) in manifest.entries {
        let Some(entries) = document["hooks"][&name].as_array_mut() else {
            mismatches.push(name);
            continue;
        };
        let matches: Vec<_> = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| **e == owned)
            .map(|(i, _)| i)
            .collect();
        if matches.len() == 1 {
            entries.remove(matches[0]);
        } else {
            mismatches.push(name);
        }
    }
    write_settings(
        &manifest.settings_path,
        &serde_json::to_vec_pretty(&document)?,
    )?;
    if mismatches.is_empty() {
        fs::remove_file(manifest_path)?;
    }
    Ok(mismatches)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_uses_exec_args_and_no_secrets() {
        let p = preview(Path::new(if cfg!(windows) {
            "C:/Program Files/赌场/bridge.exe"
        } else {
            "/opt/赌场/bridge.exe"
        }))
        .unwrap();
        assert_eq!(p.len(), 9);
        assert_eq!(p[0].1["hooks"][0]["args"], json!(["hook"]));
    }
    #[test]
    fn malformed_configuration_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.json");
        fs::write(&settings, b"{broken").unwrap();
        assert!(read_settings(&settings).is_err());
        assert_eq!(fs::read(settings).unwrap(), b"{broken");
    }
}
