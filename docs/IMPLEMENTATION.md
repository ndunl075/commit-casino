# Implementation and acceptance record

The architecture is the product specification. Each feature is reviewed, tested,
and merged independently. An implementation is not release acceptance: native
installation and provider lifecycle checks must be observed before claiming support.

| Feature | Implementation | Verification |
| --- | --- | --- |
| Canonical catalog and pure rules | 157 European bets, payouts, secure mapping, draft operations | Exhaustive 5,809 combinations; property tests; invalid input and rejection boundaries |
| Service and storage contracts | Versioned TypeScript/Zod command schemas and DTOs | Strict TypeScript and contract tests |
| Browser transactional service | In progress | Recovery, request replay and cross-tab tests pending |
| Shared game UI | In progress | Browser interaction and screenshots pending |
| Native authority and Windows package | In progress | Native tests and actual packaging pending |
| Passive Claude helper | In progress | Versioned fixtures and real lifecycle checks pending |
| Agent evidence and prediction | In progress | Separate acceptance gate; wagering stays disabled until verified |
| Release materials | Pending | CI, documentation, notices, archives and checksums |
