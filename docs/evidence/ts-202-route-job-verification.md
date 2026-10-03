# TS-202 route-job verification

Verified 2026-10-03 on `story/ts-202`.

- `job_coordinator.rs`: 5 deterministic tests cover owned/current submission, identical and
  conflicting retries, exclusive leases, lease recovery and fencing, cancellation races, provider
  budgets, deadline publication, and bounded-attempt exhaustion.
- `route_jobs.rs`: authenticated API integration covers complete validation, `202`, repeat identity,
  conflict, private not-found behavior, and idempotent cancellation.
- `shutdown.rs`: verifies provider work is not invoked when durable state denies scheduling.
- `migrations.rs`: includes a PostgreSQL two-claim race and cancellation-versus-completion check;
  it skips explicitly when no integration database is configured.
- TypeScript client decoding tests reject malformed or unknown job results.
- `npm run build && npm run factory:verify`: passed.

No route geometry, private coordinates, provider results, or success metrics are fabricated by
this story. Candidate publication consumes only candidate rows validated by their producer.
