# TS-202 route-job lifecycle handoff

Producer: TS-202  
Consumers: TS-203, TS-204, TS-205, TS-401, TS-402, TS-403  
Contract version: 1

- `POST /api/v1/route-jobs` accepts the complete immutable, versioned request under the TS-201
  guest cookie and Origin/CSRF boundary. It returns `202` and the original job for an identical
  `(session, idempotency_key, canonical input)` retry; changed input returns
  `409 idempotency_conflict`.
- Resolution ownership, expiry, profile, coverage-policy, and map-data versions are checked in the
  same transaction that creates the immutable planning request and queued job.
- `PostgresJobCoordinator` provides atomic claim (`FOR UPDATE SKIP LOCKED`), renewal, provider-call
  permission, idempotent cancellation, and fenced completion. Attempt and deadline exhaustion are
  explicit terminal states, never `no_match` or success.
- Candidate success is accepted only when every supplied candidate ID already belongs to the
  leased job. A deadline with no such candidate is `timed_out`; a valid candidate may carry
  `budget_exhausted=true`.
- Workers must call `run_provider_call` immediately before each provider operation. A cancelled,
  expired, lost, or under-budget lease returns no scheduled call.

Verify with `npm run factory:verify`. PostgreSQL concurrency coverage runs when the documented
`DATABASE_URL` integration environment is available; deterministic in-memory concurrency and
fencing tests always run.
