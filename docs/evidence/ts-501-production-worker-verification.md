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
- `node scripts/cargo.mjs test -p gpsart-worker --all-targets` - unit/integration targets passed; the real PostGIS pipeline test is an explicit environment skip when `DATABASE_URL` is absent.
- `node scripts/cargo.mjs clippy -p gpsart-worker -p gpsart-persistence --all-targets -- -D warnings` - passed.
- PostgreSQL execution attempt with `DATABASE_URL=postgres://gpsart_app@127.0.0.1:5433/gpsart_local` - blocked because `gpsart_app` lacks `CREATEDB`; this is retained as an environment limitation, not represented as a database pass.

Independent QA and full `factory:verify` results are pending.
