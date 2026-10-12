use crate::{
    invalid,
    sanitizer::{opaque, HookType, SanitizedHook},
    BridgeResult,
};
use fs2::FileExt;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
type HmacSha256 = Hmac<Sha256>;
pub const MAX_MESSAGE: u64 = 8192;
pub const MAX_PENDING: usize = 1024;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Configuration {
    pub version: u8,
    pub installation_id: String,
    pub secret: String,
    pub provider_version: String,
    pub auto_launch: bool,
    pub desktop_executable: Option<PathBuf>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Envelope {
    pub version: u8,
    pub event_id: String,
    pub installation_id: String,
    pub provider: String,
    pub source_sequence: u64,
    pub observed_at_ms: u64,
    pub turn_id: Option<String>,
    pub observation: SanitizedHook,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<crate::verifier::RunSummary>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedMessage {
    pub payload: String,
    pub signature: String,
}
#[derive(Default, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Journal {
    pub sequence: u64,
    pub turn_id: Option<String>,
    pub stopped: bool,
    pub ended: bool,
    pub evidence_gap: bool,
    pub tool_turns: BTreeMap<String, String>,
    pub link_id: Option<String>,
    #[serde(default)]
    pub gap_epoch: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchLink {
    pub installation_id: String,
    pub created_at_ms: u64,
    pub session_id: Option<String>,
}
pub fn read_link(root: &Path, link: &str) -> BridgeResult<LaunchLink> {
    if !opaque(link) {
        return Err(invalid("INVALID_LINK"));
    }
    let p = root.join("links").join(format!("{link}.json"));
    check_owned(&p)?;
    if fs::metadata(&p)?.len() > MAX_MESSAGE {
        return Err(invalid("LINK_OVERSIZED"));
    }
    let value: LaunchLink = serde_json::from_slice(&fs::read(p)?)?;
    if value.installation_id != load_config(root)?.installation_id
        || now_ms().saturating_sub(value.created_at_ms) > 86_400_000
    {
        return Err(invalid("EXPIRED_LINK"));
    }
    Ok(value)
}
pub fn note_gap(root: &Path) {
    // No provider input, secret, path or prompt is ever written to diagnostics.
    let _ = atomic_write(
        &root.join("evidence-gap.json"),
        &serde_json::to_vec(&now_ms()).unwrap_or_default(),
    );
}
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
pub fn check_owned(path: &Path) -> BridgeResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(invalid("SYMLINK_REJECTED"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("REPARSE_POINT_REJECTED"));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(invalid("PRIVATE_PERMISSIONS_REQUIRED"));
        }
    }
    Ok(())
}
pub fn private_dir(path: &Path) -> BridgeResult<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    check_owned(path)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> BridgeResult<()> {
    let parent = path.parent().ok_or_else(|| invalid("NO_PARENT"))?;
    check_owned(parent)?;
    if path.exists() {
        check_owned(path)?;
    }
    let temp = parent.join(format!("{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file); // ReplaceFileW requires the replacement handle closed on Windows.
                // Windows rename cannot replace an existing file; use ReplaceFileW for atomic replacement.
    #[cfg(windows)]
    if path.exists() {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn ReplaceFileW(
                replaced: *const u16,
                replacement: *const u16,
                backup: *const u16,
                flags: u32,
                exclude: *mut std::ffi::c_void,
                reserved: *mut std::ffi::c_void,
            ) -> i32;
        }
        let a: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let ok = unsafe {
            ReplaceFileW(
                a.as_ptr(),
                b.as_ptr(),
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        return Ok(());
    }
    fs::rename(temp, path)?;
    Ok(())
}
pub fn load_config(root: &Path) -> BridgeResult<Configuration> {
    check_owned(root)?;
    let path = root.join("config.json");
    check_owned(&path)?;
    if fs::metadata(&path)?.len() > MAX_MESSAGE {
        return Err(invalid("CONFIG_OVERSIZED"));
    }
    let c: Configuration = serde_json::from_slice(&fs::read(path)?)?;
    if c.version != 1 || !opaque(&c.installation_id) || hex::decode(&c.secret)?.len() != 32 {
        return Err(invalid("INVALID_CONFIG"));
    }
    Ok(c)
}
pub fn session_key(session: &str) -> String {
    hex::encode(Sha256::digest(session.as_bytes()))
}
pub fn journal_path(root: &Path, session: &str) -> PathBuf {
    root.join("journals")
        .join(format!("{}.json", session_key(session)))
}
pub fn lock_session(root: &Path, session: &str) -> BridgeResult<File> {
    check_owned(&root.join("journals"))?;
    let path = root
        .join("journals")
        .join(format!("{}.lock", session_key(session)));
    if path.exists() {
        check_owned(&path)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.try_lock_exclusive()
        .map_err(|_| invalid("JOURNAL_BUSY"))?;
    Ok(file)
}
pub fn load_journal(root: &Path, session: &str) -> BridgeResult<Journal> {
    let p = journal_path(root, session);
    if !p.exists() {
        return Ok(Journal::default());
    }
    check_owned(&p)?;
    if fs::metadata(&p)?.len() > 65536 {
        return Err(invalid("JOURNAL_OVERSIZED"));
    }
    Ok(serde_json::from_slice(&fs::read(p)?)?)
}
pub fn signed_bytes(envelope: &Envelope, secret: &str) -> BridgeResult<Vec<u8>> {
    let payload = serde_json::to_string(envelope)?;
    let mut mac = HmacSha256::new_from_slice(&hex::decode(secret)?)?;
    mac.update(payload.as_bytes());
    Ok(serde_json::to_vec(&SignedMessage {
        payload,
        signature: hex::encode(mac.finalize().into_bytes()),
    })?)
}
pub fn authenticate(
    bytes: &[u8],
    config: &Configuration,
    consumed_sequence: u64,
    now: u64,
) -> BridgeResult<Envelope> {
    if bytes.len() > MAX_MESSAGE as usize {
        return Err(invalid("MESSAGE_OVERSIZED"));
    }
    let signed: SignedMessage = serde_json::from_slice(bytes)?;
    let mut mac = HmacSha256::new_from_slice(&hex::decode(&config.secret)?)?;
    mac.update(signed.payload.as_bytes());
    mac.verify_slice(&hex::decode(signed.signature)?)
        .map_err(|_| invalid("BAD_SIGNATURE"))?;
    let event: Envelope = serde_json::from_str(&signed.payload)?;
    if event.version != 1
        || event.provider != "claude"
        || event.installation_id != config.installation_id
        || !opaque(&event.event_id)
        || !opaque(&event.observation.session_id)
        || event.turn_id.as_ref().is_some_and(|t| !opaque(t))
        || event.source_sequence <= consumed_sequence
        || event.observed_at_ms > now.saturating_add(30_000)
        || now.saturating_sub(event.observed_at_ms) > 86_400_000
    {
        return Err(invalid("INVALID_OR_REPLAYED_EVENT"));
    }
    Ok(event)
}
/** Caller holds this same lock when arming; never infer eligibility from the UI snapshot. */
pub fn enqueue(root: &Path, hook: SanitizedHook, link: Option<&str>) -> BridgeResult<Envelope> {
    let config = load_config(root)?;
    if config.provider_version != crate::DOCUMENTED_FIXTURE_VERSION {
        return Err(invalid("VERSION_MISMATCH"));
    }
    check_owned(&root.join("inbox"))?;
    let _lock = lock_session(root, &hook.session_id)?;
    let mut journal = load_journal(root, &hook.session_id)?;
    let gap_path = root.join("evidence-gap.json");
    if gap_path.exists() {
        check_owned(&gap_path)?;
        let epoch: u64 = serde_json::from_slice(&fs::read(&gap_path)?)?;
        if epoch > journal.gap_epoch {
            journal.evidence_gap = true;
            journal.gap_epoch = epoch;
        }
    }
    let prior_gap = journal.evidence_gap;
    // Persist uncertainty before allocating sequence/publication: watchdog interruption fails closed.
    journal.evidence_gap = true;
    atomic_write(
        &journal_path(root, &hook.session_id),
        &serde_json::to_vec(&journal)?,
    )?;
    journal.evidence_gap = prior_gap;
    if fs::read_dir(root.join("inbox"))?
        .take(MAX_PENDING + 1)
        .count()
        >= MAX_PENDING
    {
        return Err(invalid("SPOOL_FULL"));
    }
    if hook.kind == HookType::SessionStart {
        if journal.sequence == 0 || journal.ended {
            journal.evidence_gap = false;
        }
        journal.ended = false;
    }
    if let Some(l) = link.filter(|l| opaque(l)) {
        let mut attribution = read_link(root, l)?;
        if attribution
            .session_id
            .as_ref()
            .is_some_and(|id| id != &hook.session_id)
        {
            return Err(invalid("MULTI_SESSION_LINK"));
        }
        if attribution.session_id.is_none() && hook.kind != HookType::SessionStart {
            return Err(invalid("SESSION_LINK_BOUNDARY_REQUIRED"));
        }
        attribution.session_id = Some(hook.session_id.clone());
        atomic_write(
            &root.join("links").join(format!("{l}.json")),
            &serde_json::to_vec(&attribution)?,
        )?;
        if journal.link_id.as_ref().is_some_and(|old| old != l) {
            return Err(invalid("LINK_CONFLICT"));
        }
        journal.link_id = Some(l.into());
    }
    journal.sequence = journal
        .sequence
        .checked_add(1)
        .ok_or_else(|| invalid("SEQUENCE_OVERFLOW"))?;
    match hook.kind {
        HookType::TurnStart => {
            if journal.turn_id.is_some() && !journal.stopped {
                journal.evidence_gap = true;
            }
            journal.turn_id = Some(Uuid::new_v4().to_string());
            journal.stopped = false;
        }
        HookType::ToolStart => {
            if journal.tool_turns.len() >= 128 {
                return Err(invalid("TOO_MANY_TOOLS"));
            }
            if let (Some(tool), Some(turn)) = (&hook.tool_id, &journal.turn_id) {
                if journal
                    .tool_turns
                    .insert(tool.clone(), turn.clone())
                    .is_some()
                {
                    journal.evidence_gap = true;
                }
            } else {
                journal.evidence_gap = true;
            }
            journal.stopped = false;
        }
        HookType::ToolEnd => {
            if hook
                .tool_id
                .as_ref()
                .and_then(|id| journal.tool_turns.remove(id))
                != journal.turn_id
            {
                journal.evidence_gap = true;
            }
            journal.stopped = false;
        }
        HookType::StopCandidate => journal.stopped = true,
        HookType::SessionEnd => journal.ended = true,
        _ => {}
    }
    // An existing gap remains until a new session boundary; successful enqueue alone cannot heal it.
    if hook.kind != HookType::SessionStart {
        journal.evidence_gap |= prior_gap;
    }
    let envelope = Envelope {
        version: 1,
        event_id: Uuid::new_v4().to_string(),
        installation_id: config.installation_id.clone(),
        provider: "claude".into(),
        source_sequence: journal.sequence,
        observed_at_ms: now_ms(),
        turn_id: journal.turn_id.clone(),
        observation: hook,
        verification: None,
    };
    let intended_gap = journal.evidence_gap;
    journal.evidence_gap = true;
    atomic_write(
        &journal_path(root, &envelope.observation.session_id),
        &serde_json::to_vec(&journal)?,
    )?;
    let bytes = signed_bytes(&envelope, &config.secret)?;
    atomic_write(
        &root
            .join("inbox")
            .join(format!("{}.json", envelope.event_id)),
        &bytes,
    )?;
    journal.evidence_gap = intended_gap;
    atomic_write(
        &journal_path(root, &envelope.observation.session_id),
        &serde_json::to_vec(&journal)?,
    )?;
    Ok(envelope)
}
/** Publish a structured run observation while caller holds the per-session lock. */
pub fn publish_verification(
    root: &Path,
    session: &str,
    journal: &mut Journal,
    summary: &mut crate::verifier::RunSummary,
    started: bool,
) -> BridgeResult<()> {
    let config = load_config(root)?;
    if fs::read_dir(root.join("inbox"))?
        .take(MAX_PENDING + 1)
        .count()
        >= MAX_PENDING
    {
        return Err(invalid("SPOOL_FULL"));
    }
    let intended_gap = journal.evidence_gap;
    journal.evidence_gap = true;
    atomic_write(&journal_path(root, session), &serde_json::to_vec(journal)?)?;
    journal.sequence = journal
        .sequence
        .checked_add(1)
        .ok_or_else(|| invalid("SEQUENCE_OVERFLOW"))?;
    if started {
        summary.started_sequence = journal.sequence;
    } else {
        summary.completed_sequence = journal.sequence;
    }
    let e = Envelope {
        version: 1,
        event_id: Uuid::new_v4().to_string(),
        installation_id: config.installation_id.clone(),
        provider: "claude".into(),
        source_sequence: journal.sequence,
        observed_at_ms: now_ms(),
        turn_id: Some(summary.turn_id.clone()),
        observation: SanitizedHook {
            session_id: session.into(),
            kind: if started {
                HookType::VerifiedRunStarted
            } else {
                HookType::VerifiedRunCompleted
            },
            tool_id: None,
            effect: None,
            attention: None,
        },
        verification: Some(summary.clone()),
    };
    atomic_write(&journal_path(root, session), &serde_json::to_vec(journal)?)?;
    atomic_write(
        &root.join("inbox").join(format!("{}.json", e.event_id)),
        &signed_bytes(&e, &config.secret)?,
    )?;
    journal.evidence_gap = intended_gap;
    atomic_write(&journal_path(root, session), &serde_json::to_vec(journal)?)?;
    Ok(())
}
/** A native consumer MUST atomically store events and its watermark before acknowledge.
 * A watcher is merely a wakeup; periodically rescan. A missing sequence never advances. */
pub fn scan_inbox(
    root: &Path,
    session: &str,
    consumed_sequence: u64,
) -> BridgeResult<Vec<Envelope>> {
    let config = load_config(root)?;
    check_owned(&root.join("inbox"))?;
    let mut by_sequence = BTreeMap::<u64, Envelope>::new();
    for entry in fs::read_dir(root.join("inbox"))?.take(MAX_PENDING + 1) {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        check_owned(&path)?;
        if fs::metadata(&path)?.len() > MAX_MESSAGE {
            return Err(invalid("MESSAGE_OVERSIZED"));
        }
        let e = authenticate(&fs::read(&path)?, &config, 0, now_ms())?;
        if path.file_stem().and_then(|s| s.to_str()) != Some(e.event_id.as_str()) {
            return Err(invalid("FILE_ID_MISMATCH"));
        }
        if e.observation.session_id != session || e.source_sequence <= consumed_sequence {
            continue;
        }
        if by_sequence.insert(e.source_sequence, e).is_some() {
            return Err(invalid("CONFLICTING_SEQUENCE"));
        }
    }
    if by_sequence.len() > MAX_PENDING {
        return Err(invalid("SPOOL_FULL"));
    }
    let mut next = consumed_sequence
        .checked_add(1)
        .ok_or_else(|| invalid("SEQUENCE_OVERFLOW"))?;
    let mut events = vec![];
    while let Some(event) = by_sequence.remove(&next) {
        events.push(event);
        next = next
            .checked_add(1)
            .ok_or_else(|| invalid("SEQUENCE_OVERFLOW"))?;
    }
    Ok(events)
}
pub fn discover_sessions(root: &Path) -> BridgeResult<Vec<String>> {
    let config = load_config(root)?;
    check_owned(&root.join("inbox"))?;
    let mut sessions = std::collections::BTreeSet::new();
    let entries: Vec<_> = fs::read_dir(root.join("inbox"))?
        .take(MAX_PENDING + 1)
        .collect();
    if entries.len() > MAX_PENDING {
        return Err(invalid("SPOOL_FULL"));
    }
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        check_owned(&path)?;
        if fs::metadata(&path)?.len() > MAX_MESSAGE {
            return Err(invalid("MESSAGE_OVERSIZED"));
        }
        let event = authenticate(&fs::read(&path)?, &config, 0, now_ms())?;
        if path.file_stem().and_then(|s| s.to_str()) != Some(event.event_id.as_str()) {
            return Err(invalid("FILE_ID_MISMATCH"));
        }
        sessions.insert(event.observation.session_id);
    }
    Ok(sessions.into_iter().collect())
}
pub fn replayed_events(
    root: &Path,
    session: &str,
    durable_consumed_sequence: u64,
) -> BridgeResult<Vec<Envelope>> {
    let config = load_config(root)?;
    check_owned(&root.join("inbox"))?;
    let mut events = vec![];
    for entry in fs::read_dir(root.join("inbox"))?.take(MAX_PENDING) {
        let p = entry?.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        check_owned(&p)?;
        if fs::metadata(&p)?.len() > MAX_MESSAGE {
            return Err(invalid("MESSAGE_OVERSIZED"));
        }
        let e = authenticate(&fs::read(&p)?, &config, 0, now_ms())?;
        if p.file_stem().and_then(|s| s.to_str()) != Some(e.event_id.as_str()) {
            return Err(invalid("FILE_ID_MISMATCH"));
        }
        if e.observation.session_id == session && e.source_sequence <= durable_consumed_sequence {
            events.push(e);
        }
    }
    Ok(events)
}
pub fn acknowledge(
    root: &Path,
    event: &Envelope,
    durable_consumed_sequence: u64,
) -> BridgeResult<()> {
    if durable_consumed_sequence < event.source_sequence {
        return Err(invalid("UNCOMMITTED_ACK"));
    }
    if !opaque(&event.event_id) {
        return Err(invalid("INVALID_ID"));
    }
    let path = root.join("inbox").join(format!("{}.json", event.event_id));
    if path.exists() {
        check_owned(&path)?;
        fs::remove_file(path)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Configuration {
        Configuration {
            version: 1,
            installation_id: "install".into(),
            secret: hex::encode([7; 32]),
            provider_version: crate::DOCUMENTED_FIXTURE_VERSION.into(),
            auto_launch: false,
            desktop_executable: None,
        }
    }
    #[test]
    fn signature_replay_and_tamper() {
        let e = Envelope {
            version: 1,
            event_id: "e1".into(),
            installation_id: "install".into(),
            provider: "claude".into(),
            source_sequence: 1,
            observed_at_ms: 100,
            turn_id: None,
            observation: crate::sanitizer::sanitize(
                br#"{"session_id":"s","hook_event_name":"Stop"}"#,
            )
            .unwrap(),
            verification: None,
        };
        let b = signed_bytes(&e, &config().secret).unwrap();
        assert!(authenticate(&b, &config(), 0, 100).is_ok());
        assert!(authenticate(&b, &config(), 1, 100).is_err());
        let mut bad: SignedMessage = serde_json::from_slice(&b).unwrap();
        bad.payload.push(' ');
        assert!(authenticate(&serde_json::to_vec(&bad).unwrap(), &config(), 0, 100).is_err());
    }
}
