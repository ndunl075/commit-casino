# Implementation and acceptance record

The architecture is the product specification. Each feature is reviewed, tested,
and merged independently. An implementation is not release acceptance: native
installation and provider lifecycle checks must be observed before claiming support.

| Feature | Implementation | Verification |
| --- | --- | --- |
| Canonical catalog and pure rules | 157 European bets, payouts, secure mapping, draft operations | Exhaustive 5,809 combinations; property tests; invalid input and rejection boundaries |
| Service and storage contracts | Versioned TypeScript/Zod command schemas and DTOs | Strict TypeScript and contract tests |
| Browser transactional service | IndexedDB authority, serialized transactions, ledger, recovery, explicit temporary mode | 22 storage tests, cross-tab conflicts, replay and injected write failures |
| Shared game UI | Astra React/Pixi table, wheel, keyboard picker, audio, settings, detailed history/statistics | 157 direct targets; four browser end-to-end flows; nine isolated UI regressions; compact/standard screenshots |
| Native authority | Rust/SQLite wallet, migrations, ledger, outcomes, settings and window commands | 18 wallet tests including 5,809 native payout combinations; strict Clippy/formatting; native inbox tests |
| Passive Claude helper | Compiled bounded signed inbox, explicit hook preview/install/disconnect, optional auto-open | Privacy/signature/replay/watchdog/ownership tests; actual Claude 2.1.287 isolated lifecycle checks |
| Agent evidence and prediction | Locked pre-prompt arming, attributed structured checks, conservative boundaries, fixed odds, refunds and recovery | Shared Rust/TS policy fixtures; actual pinned runner processes; native registered Vitest→signed inbox→SQLite payout/restart |
| Windows distribution | Offline WebView2 NSIS installer, compiled helper and license inventory | Actual WebView2 UI→IPC→SQLite smoke; settled restart and process kill during unshown spin recover recorded payout once |
| Release materials | CI, documentation, notices, browser/helper/installer archives and checksums | Local reproducible scripts; GitHub tagged-release workflow; macOS/Linux deferred until platform installation testing |

## Tested environment and scope

Windows x64, Node 22.23.2, Rust 1.99 MSVC, Claude Code 2.1.287, Vitest 5.0.3,
and Playwright 1.64.0. Native tests use isolated wallets and temporary owned
integration settings. No user prompts/source/transcripts, permission bypasses,
paid inference, runtime gameplay network requests, or user hook changes are
required by acceptance checks.

The actual Claude CLI lifecycle harness uses a local mocked Anthropic endpoint:
provider hooks are real, model responses are test fixtures. It verifies lifecycle
transport and permission behavior; it does not claim a live model's task quality.
Pure fixtures cover canceled/parallel/stale reports, resumed attention, sequence
gaps, subagents, late events and classifier precedence. Real runner harnesses
preserve success and intentional failure exits and discard raw private reports.

The Windows smoke executable has a compile-time test feature for storage
isolation. Production builds exclude it. It uses real product randomness and
checks the recorded payout; no test-only result setter is exposed. A local
one-second native animation sample recorded 181 animation callbacks averaging
5.53 ms on the test machine; this is frame pacing evidence, not a GPU benchmark
or a performance guarantee for all hardware.

All 111 current TypeScript unit/contract tests pass, along with ESLint, strict
TypeScript, native tests, Rust formatting and strict Clippy. Browser Playwright
checks cover offline reload after initial cache installation, local persistence,
draft operations, payout arithmetic and every direct table target. Graphics-loss,
hidden-wheel, stale async responses, observer drafts and retry safety have
separate UI regression cases.

Final audit regressions cover a shell changing files after its contained tests
and unavailable provider storage. Mutating tool completion invalidates earlier
contained checks; dedicated verification runs after tools finish. Native timeout
and explicit refunds remain available when helper storage fails. The actual
installer was installed into an owned test directory, launched with no Node or
Rust on its runtime PATH, checked for its bundled helper/notices, and uninstalled
successfully while preserving wallet data.

Unsigned installer prompts are expected. First-time offline installation bundles
WebView2; users do not need development tools. No macOS/Linux release is claimed.
