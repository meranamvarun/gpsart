# TS-204 planner workflow verification

## Acceptance checks

1. `tests/start-selection.test.ts` proves manual coordinates, selected search data, map data, and explicit geolocation converge on `ProposedStart`; denial, timeout, unsupported access, and low accuracy remain explicit.
2. `tests/planner-workflow.test.ts` proves no job is created before exact start confirmation, invalid input and unsupported coverage create no job, and concurrent or sequential duplicate submissions preserve one logical idempotency key. An ambiguous creation retry, including after refresh, replays the exact confirmed serialized body and key before newer work; tampered pending records are rejected without a network call.
3. The workflow and screen tests prove async revision fencing, refresh recovery with a strictly validated cached representation and ETag, authoritative job expiry, bounded polling, cancellation fencing, active-request progress labels, and retention of an older terminal response as `previousStatus`.
4. `tests/guest-session.test.ts` proves a refresh reuses unexpired public session policy instead of issuing replacement ownership credentials.
5. `crates/gpsart-api/tests/route_jobs.rs`, the OpenAPI contract, and the migration tests prove `return_to_start` is required and persisted. Upgrade coverage proves historical requests remain explicitly unknown.
6. The native controller connects form submission, map selection, on-demand geolocation, confirmation, observed stage display, cancellation, session-ready refresh recovery, terminal outcomes, and stale-result protection without synthesizing route data.

## Verification results

- Focused browser/domain suite: 21 passed.
- Complete TypeScript unit suite: 42 passed.
- Visual-shell regression suite: 2 passed.
- Production TypeScript/Vite build: passed.
- Rust route-job API integration test: passed.
- Factory verification: passed, including Rust format, clippy, all workspace tests/builds, factory contracts, browser tests, Sites tests, and production build.
