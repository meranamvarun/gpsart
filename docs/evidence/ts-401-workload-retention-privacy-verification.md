# TS-401 workload, retention, and privacy verification

Verified 2026-10-11.

## Acceptance checks

1. Configuration tests enumerate every expensive dimension and reject retention over 24 hours, runtime over 60 seconds, and invalid concurrency relationships.
2. API abuse tests cover unsupported tolerance, oversized JSON (`413`), rate limiting (`429` plus retry guidance), and idempotency behavior. PostgreSQL integration covers atomic per-guest admission and proves a new idempotency key cannot bypass the active-job cap.
3. Migration 0008 durably counts provider calls. Cleanup integration covers live-lease protection, expiry deletion, 100-record batching, and explicit retry state.
4. `tests/privacy-contract.test.mjs` audits structured application events for protected field names and verifies secret debug redaction.
5. `docs/privacy-retention-policy.md` defines application/proxy/error/analytics exclusions, server-only provider credentials, public-key origin restrictions, live deletion, and seven-day encrypted-backup deletion.

## Results

- Focused configuration, API abuse, guest-session, migration, cleanup, and provider-budget tests passed. PostgreSQL-specific cases run when `DATABASE_URL` is available and skip explicitly otherwise.
- Workspace strict clippy passed after implementation.
- `npm run factory:verify` passed on 2026-10-11, including formatting, strict clippy, all workspace tests, all TypeScript/factory/visual/Sites tests, and the production build.
- Independent QA reviewed `3f6f9c6..7953eb5`, reproduced and fixed the tolerance-overflow, unscheduled-cleanup, and session-cascade batching defects recorded as INC-042 through INC-044, and reran the full factory verification successfully.
