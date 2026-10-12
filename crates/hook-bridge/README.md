# Passive bridge and evidence policy

The compiled `commit-casino-bridge.exe` is bundled with the Windows application. It never interprets prompts, reads source/transcripts, or returns approval decisions. Command hooks emit no stdout/stderr and always exit zero. A 250 ms process watchdog bounds stdin and disk stalls. The private spool has 1,024 entries maximum; failures preserve uncertainty rather than blocking Claude.

Windows Claude **2.1.287** was checked locally using the documented `--init-only` lifecycle in an isolated configuration directory: actual `SessionStart` and `SessionEnd` hooks traversed the authenticated spool, using an executable path with spaces and non-ASCII characters. No conversation or external inference was started. Other turn/tool/attention shapes currently have synthetic compatibility tests. The supported version is pinned; changing it requires fixture updates and actual compatibility checks.

Additional real CLI lifecycle acceptance uses a **local mocked Anthropic HTTP API**, not an external model. `node crates/hook-bridge/tests/mock-claude-lifecycle.mjs ABSOLUTE_CLAUDE_EXE` (after building the helper) creates isolated settings, uses a private synthetic text file, pins the base URL to localhost and rejects external proxy targets. On Claude2.1.287 it verified actual UserPromptSubmit, PreToolUse, PostToolUse success, PostToolUseFailure, Stop and SessionEnd. A restrictive temporary `permissions.ask` policy also exercised PermissionRequest without any helper decision output. All responses were mocked, all sessions exited0, signatures/sequences/privacy passed and no pending hook tickets remained. The provider omitted tool-end after noninteractive permission denial; this is conservatively Unknown/refund when native accounting remains incomplete. Notification timing, interactive permission resume and subagent/stop-continuation behavior still rely on conservative fixtures rather than this live check.

Default integration data: `%LOCALAPPDATA%/CommitCasino/integration`. Explicit setup creates a fresh tree with inheritance disabled and access limited to the current SID. The random installation secret exists only in that tree; it is absent from hook commands, exports and logs. Symlinks/reparse points are rejected. Same-user malicious processes are outside the recreational integrity guarantee.

The desktop uses the Rust library for detection, exact setup preview, installation, disconnect and authenticated inbox scanning. Settings ownership is recorded in a manifest and backup. Malformed configuration is refused. Disconnect removes exactly matching owned entries; edited/missing/ambiguous entries are preserved and reported. Project-local `.claude/settings.local.json` is the default desktop scope; no settings edit happens before explicit setup.

Explicit command-line operations:

```
commit-casino-bridge init 2.1.287
commit-casino-bridge preview
commit-casino-bridge install ABSOLUTE_SELECTED_SETTINGS_PATH
commit-casino-bridge disconnect ABSOLUTE_OWNED_MANIFEST_PATH
commit-casino-bridge register-claude ABSOLUTE_CLAUDE_EXE
commit-casino-bridge launch-claude [ordinary Claude arguments]
commit-casino-bridge register-suite ABSOLUTE_SUITE_JSON
commit-casino-bridge require-suites SUITE_ID [OTHER_REQUIRED_SUITE_IDS]
commit-casino-bridge verify SUITE_ID
```

`launch-claude` passes a random private launch link through the environment, preserves the child exit, and adds no prompt or permission instructions. The SessionStart hook binds the link to exactly one provider session. `verify` resolves that session and freezes its active turn under the boundary lock before launching an explicitly registered command. Neither a hook nor the frontend can supply executable commands. Suites launch only when a person explicitly invokes the wrapper.

Suite registration schema (use the actual absolute executable and CLI paths):

```json
{
  "suiteId": "unit",
  "runner": "vitest",
  "runnerVersion": "5.0.3",
  "executable": "C:/Program Files/nodejs/node.exe",
  "args": ["C:/selected-project/node_modules/vitest/vitest.mjs", "run"],
  "versionArgs": ["C:/selected-project/node_modules/vitest/vitest.mjs", "--version"],
  "workingDirectory": "C:/selected-project"
}
```

Supported report profiles are Vitest 5.0.3 JSON and Playwright 1.64.0 JSON. The wrapper owns reporter/output/retry flags, disables retries, verifies the actual version, and uses a new UUID private run directory. It preserves stdout/stderr and actual process exit even if report verification fails. Only counts, status, timing, opaque suite/run/session/turn IDs and sequences are retained; raw reports are deleted after normalization. Missing/stale/zero-test/conflicting/flaky/unsupported reports cannot establish success. A killed wrapper leaves an incomplete started observation; it cannot silently reuse an earlier pass.

`require-suites` validates already registered suites and writes private `required-suites.json` containing `{ "suiteIds": ["unit"] }`. The native authority must require a nonempty validated policy before arming and freeze those IDs with the wager. Reconfiguration cannot rewrite the policy of an existing obligation.

Consumers periodically scan as well as watch the inbox. Only contiguous authenticated sequences advance. Persist accepted events and the consumed watermark in the same database transaction before deleting files. `replayed_events` permits cleanup after a crash between database commit and deletion. `Journal.evidence_gap` and the durable preallocation gap epoch must also be checked; absence of numeric gaps is insufficient. Arming future wagers must take the same session lock, drain boundaries, and validate idle eligibility before debit.

Each hook flushes an opaque ticket under `pending-hooks` before reading stdin. It removes the ticket only after successful signed publication. A stalled/killed hook therefore blocks Agent coverage even when no sequence was allocated. `load_journal` overlays pending tickets and a newer global drop epoch as `evidence_gap`; native callers must reject arming while either is present. Do not delete abandoned tickets automatically to manufacture continuity. Recovering a failed installation requires an explicit fresh coverage setup after outstanding wagers have been refunded.

Automatic opening is off by default. When explicitly enabled, a private native heartbeat (`desktop-state.json` with `lastSeenMs` and `pid`), per-session suppression (`suppressed-sessions.json` with `sessionIds`), and three-second launch cooldown prevent process bursts. Only a fresh accepted main prompt can start the fixed configured desktop executable; native code applies non-focus window presentation.

Native checks: `cargo test --manifest-path crates/hook-bridge/Cargo.toml`. After `npm ci`, run `cargo test --manifest-path crates/hook-bridge/Cargo.toml --test registered_runner -- --ignored --test-threads=1` to verify the actual pinned local Vitest and Playwright processes. This harness passed on Windows for success and intentional failed assertions: actual exits 0/1, valid normalized counts, continuous authenticated run sequences, raw report deletion and summary privacy. It uses explicitly simulated turn metadata and does not start an AI conversation. Playwright checks use ordinary API assertions without launching a browser or network request.

**Agent availability is decided by native authority for each session.** The pure classifier and published 1:1 odds/refunds are tested against shared Rust/TypeScript fixtures; actual local runners and Claude lifecycle transport have separate acceptance harnesses. The desktop financial authority also requires continuous coverage, a private launch link, a validated nonempty required-suite policy, an eligible idle session, pre-prompt locked binding and idempotent settlement. Browser Agent wagering remains unavailable. Passive notifications and Classic gameplay remain independent of this gate. Unsupported, incomplete or unattributed observations refund Unknown rather than invent success.

Official API references checked during implementation: [Claude hook reference](https://code.claude.com/docs/en/hooks), [Vitest reporters](https://vitest.dev/guide/reporters), [Playwright reporters](https://playwright.dev/docs/test-reporters). The current Claude reference explicitly supports direct `command` + `args` execution without a shell.
