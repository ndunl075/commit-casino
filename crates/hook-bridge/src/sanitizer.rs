use crate::{invalid, BridgeResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_INPUT: usize = 64 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HookType {
    SessionStart,
    SessionEnd,
    TurnStart,
    ToolStart,
    ToolEnd,
    AttentionRequested,
    StopCandidate,
    VerifiedRunStarted,
    VerifiedRunCompleted,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ToolEffect {
    Read,
    PossibleMutation,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SanitizedHook {
    pub session_id: String,
    #[serde(rename = "type")]
    pub kind: HookType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<ToolEffect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention: Option<String>,
}
pub fn opaque(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub fn sanitize(bytes: &[u8]) -> BridgeResult<SanitizedHook> {
    if bytes.len() > MAX_INPUT {
        return Err(invalid("OVERSIZED_INPUT"));
    }
    let raw: Value = serde_json::from_slice(bytes)?;
    if raw.get("agent_id").is_some() || raw.get("agent_type").is_some() {
        return Err(invalid("AMBIGUOUS_SUBAGENT"));
    }
    let session_id = raw["session_id"]
        .as_str()
        .filter(|s| opaque(s))
        .ok_or_else(|| invalid("INVALID_SESSION"))?
        .to_owned();
    let mut hook = SanitizedHook {
        session_id,
        kind: HookType::SessionStart,
        tool_id: None,
        effect: None,
        attention: None,
    };
    hook.kind = match raw["hook_event_name"].as_str() {
        Some("SessionStart") => HookType::SessionStart,
        Some("SessionEnd") => HookType::SessionEnd,
        Some("UserPromptSubmit") => HookType::TurnStart,
        Some("Stop") => HookType::StopCandidate,
        Some("PermissionRequest") => {
            hook.attention = Some("permission".into());
            HookType::AttentionRequested
        }
        Some("Notification") => {
            hook.attention = Some(
                match raw["notification_type"].as_str() {
                    Some("permission_prompt") => "permission",
                    Some("idle_prompt") => "idle",
                    _ => return Err(invalid("UNSUPPORTED_NOTIFICATION")),
                }
                .into(),
            );
            HookType::AttentionRequested
        }
        Some(event @ ("PreToolUse" | "PostToolUse" | "PostToolUseFailure")) => {
            hook.tool_id = Some(
                raw["tool_use_id"]
                    .as_str()
                    .filter(|s| opaque(s))
                    .ok_or_else(|| invalid("INVALID_TOOL_ID"))?
                    .to_owned(),
            );
            hook.effect = Some(match raw["tool_name"].as_str() {
                Some("Read" | "Glob" | "Grep" | "WebFetch" | "WebSearch") => ToolEffect::Read,
                _ => ToolEffect::PossibleMutation,
            });
            if event == "PreToolUse" {
                HookType::ToolStart
            } else {
                HookType::ToolEnd
            }
        }
        _ => return Err(invalid("UNSUPPORTED_EVENT")),
    };
    Ok(hook)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn privacy_before_storage() {
        let s = sanitize(br#"{"session_id":"s1","hook_event_name":"UserPromptSubmit","prompt":"SECRET","cwd":"SECRET","transcript_path":"SECRET"}"#).unwrap();
        assert!(!serde_json::to_string(&s).unwrap().contains("SECRET"));
        assert_eq!(s.kind, HookType::TurnStart);
    }
    #[test]
    fn reject_subagents_unknown_and_oversize() {
        assert!(
            sanitize(br#"{"session_id":"s","hook_event_name":"Stop","agent_id":"x"}"#).is_err()
        );
        assert!(sanitize(br#"{"session_id":"s","hook_event_name":"TaskCompleted"}"#).is_err());
        assert!(sanitize(&vec![b' '; MAX_INPUT + 1]).is_err());
    }
}
