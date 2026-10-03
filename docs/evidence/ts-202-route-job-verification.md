# TS-202 route-job verification

Verified 2026-10-03 on `story/ts-202`.

- `job_coordinator.rs`: 6 deterministic tests cover owned/current submission, identical and
  conflicting retries, exclusive leases, lease recovery and fencing, cancellation races, provider
  budgets, premature and deadline timeout behavior, and bounded-attempt exhaustion.
- `starts.rs`: 7 API tests include durable confirmation recording and an explicit
  `503 storage_unavailable` result. `route_jobs.rs` covers strict complete validation, published
  template selection, distance bounds, `202`, repeat identity, conflict, private not-found
  behavior, and idempotent cancellation.
- `shutdown.rs`: 3 tests prove durable denial invokes no provider work and the provider task is
  scheduled while the cancellation-ordering permit remains held.
- `migrations.rs`: 10 conditional PostgreSQL cases include start-confirmation-to-job integration,
  a two-claim race, cancellation-versus-completion, and provider-permit/cancellation ordering. The
  candidate-publication case also rejects map/algorithm version drift before the deadline and
  permits only timeout at the deadline. Bodies skip explicitly when no integration database is
  configured on this host.
- 21 TypeScript unit tests pass; route-job decoding rejects malformed UUIDs, unknown fields, and
  non-scalar error codes.
- Focused Rust and TypeScript checks passed after independent-review fixes. The final
  `npm run build && npm run factory:verify` also passed on 2026-10-03 and is recorded in
  `factory/stories.json`.

No route geometry, private coordinates, provider results, or success metrics are fabricated by
this story. Candidate publication consumes only candidate rows validated by their producer.
