# TS-501 production worker verification

Date: 17 October 2026

## Acceptance evidence

1. `ClaimedJobInput` is loaded only through a live fenced lease and repeats the immutable owner/start/request/version relationships. Malformed legacy finish data, unknown versions, expired inputs, stale workers and inconsistent start pins fail closed.
2. The executable loads an external SHA-256-pinned `LocalOsmGraph`; the production `take_work` operation delegates to `ProductionWorker::take_one` rather than a no-op. Heart and curated-text use the real `CandidateSearch`, durable provider-call permits and `score_candidate` validation.
3. `publish_success` atomically publishes one validated candidate snapshot and the terminal success outcome. Its PostgreSQL regression proves a stale lease leaves zero candidate rows, a live lease publishes one result, and duplicate delivery receives `Terminal`.
4. Search renews the lease every second. Lease/cancellation/deadline loss cancels search and cannot publish. Database failure returns to the reconnect loop. Graceful shutdown interrupts pending work, while expired leases remain recoverable under the existing bounded-attempt claim contract.
5. Queue, stage, search, provider and terminal metrics arise from actual execution and carry only job/attempt correlation, coarse result classes, and previously approved dimensions; no coordinate, trace, token, cookie, credential or custom text is logged.
6. Focused coverage includes immutable-input validation, real heart/text pipeline setup, atomic success/stale/duplicate behavior, cancellation/provider budget gates, reconnect, and shutdown. Existing job coordinator and routing suites cover timeout, no-match, attempt exhaustion and invalid provider results.

## Commands run

- `node --test tests/production-worker-contract.test.mjs` - 2 passed.
- `DATABASE_URL=postgres://postgres@127.0.0.1:55432/postgres node scripts/cargo.mjs test -p gpsart-worker --all-targets -- --test-threads=1` - 12 passed, including 3 production-worker/PostgreSQL cases.
- `DATABASE_URL=postgres://postgres@127.0.0.1:55432/postgres node scripts/cargo.mjs test -p gpsart-persistence --test migrations -- --test-threads=1` - 18 passed, including populated-v4 upgrade, stale lease, atomic publication, duplicate delivery, timeout, cancellation and durable budget cases.
- `node scripts/cargo.mjs clippy -p gpsart-worker -p gpsart-persistence --all-targets -- -D warnings` - passed.
- `DATABASE_URL=postgres://postgres@127.0.0.1:55432/postgres npm run factory:verify` - passed in 62.2 seconds with PostgreSQL-dependent tests executed (17 October 2026).

Independent QA rejected the initial implementation and identified immutable-start, terminal-classification,
database-failure, lease-race, configured-limit, clean-checkout, documentation, confirmed-start, migration-
compatibility, restart, duplicate-delivery and publication-failure evidence defects. The final review by
`codex/qa_ts501_final` approved all six acceptance criteria after independently passing the 18-case
PostgreSQL migration suite and 12-case worker suite. INC-052 and INC-053 retain the confirmed-start and
populated-v4 upgrade defects with permanent regression checks. Historical migration 0005 remains byte-
identical to `origin/main`; the forward-only migration 0009 and supported migration runner repair legacy
upgrades without invalidating SQLx checksums.
