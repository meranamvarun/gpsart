# TS-401 workload, retention, and privacy handoff

**Version:** 1  
**Producer:** TS-401  
**Consumers:** TS-402, TS-403, TS-404  
**Compatibility:** additive migration 0008; new `429`/`413` API errors

## Contract

- `WorkloadLimits` owns the validated distance, tolerance, template, anchor, attempt, provider-call, payload, runtime, concurrency, submission-rate, and retention caps. Environment overrides may tighten limits but cannot extend runtime beyond 60 seconds, retention beyond 24 hours, or per-guest concurrency above deployment concurrency.
- PostgreSQL admission serializes global decisions, returns an existing idempotent job before applying new-work limits, and returns `JobError::RateLimited { retry_after_seconds }` for rejected new work. The API maps that to `429`, `Retry-After`, and a machine-readable body.
- Provider-call counts are durable on `generation_jobs` and increment inside the locked scheduling transaction.
- The API process invokes cleanup immediately and every 30 seconds until shutdown. Cleanup uses batches of 100, never deletes a request family behind a live lease or bypasses batching through session cascades, records count-only run evidence, and indicates whether another batch is required.
- Production structured logs cannot contain protected coordinate, geometry, credential, cookie, token, or future custom-text fields. The detailed sink, backup, and provider-key policy is in `docs/privacy-retention-policy.md`.

## Verification

```text
npm run factory:verify
```

Focused checks:

```text
cargo test -p gpsart-config --test configuration
cargo test -p gpsart-api --test route_jobs
cargo test -p gpsart-persistence --test migrations cleanup_is_batched_and_never_deletes_work_behind_an_active_lease
cargo test -p gpsart-persistence --test migrations provider_call_budget_is_durable_and_hard_capped
node --test tests/privacy-contract.test.mjs
```
