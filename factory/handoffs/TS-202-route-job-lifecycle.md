# TS-202 route-job lifecycle handoff

Producer: TS-202  
Consumers: TS-203, TS-204, TS-205, TS-401, TS-402, TS-403  
Contract version: 1

- `POST /api/v1/route-jobs` accepts the complete immutable, versioned request under the TS-201
  guest cookie and Origin/CSRF boundary. It returns `202` and the original job for an identical
  `(session, idempotency_key, canonical input)` retry; changed input returns
  `409 idempotency_conflict`. The accepted response contains only `job_id` and
  `request_revision`, because an identical retry may refer to a job that is no longer queued.
- Validation rejects unknown top-level or profile fields, distances outside 1,000–100,000 metres,
  tolerance above the requested distance, invalid profile/version pairs, and any template/version
  that is not in the published embedded catalogue. `algorithm_version` is bound to the build's
  supported `algorithm-v1` contract rather than trusting a client label. OpenAPI and the TypeScript
  decoder enforce the same boundary, including strict version-4 job UUIDs and scalar error codes.
- Production start confirmation writes `resolved_starts` through `PostgresResolvedStartStore`
  before returning success. The stored requested/confirmed coordinates, owner, expiry, profile,
  coverage-policy, and map-data versions are the exact record consumed by job authorization;
  persistence failure is explicitly `storage_unavailable`, never a routing-provider failure.
- Resolution ownership, expiry, profile, coverage-policy, and map-data versions are checked in the
  same transaction that creates the immutable planning request and queued job.
- `PostgresJobCoordinator` provides atomic claim (`FOR UPDATE SKIP LOCKED`), renewal, provider-call
  permission, idempotent cancellation, and fenced completion. Attempt and deadline exhaustion are
  explicit terminal states, never `no_match` or success.
- Candidate success is accepted only when every supplied candidate ID already belongs to the
  leased job and its immutable map-data and algorithm versions match the planning request. A
  deadline with no such candidate is `timed_out`; invalid publication before the deadline is
  rejected, and a valid candidate may carry `budget_exhausted=true`. TS-205 owns the producer-side
  route-validity checks before a row is eligible for publication.
- Workers must call `run_provider_call` immediately before each provider operation. Its durable
  permit holds the job row lock while the closure creates a Tokio `JoinHandle`; this makes task
  scheduling atomic with cancellation. The lock is released before awaiting the in-flight task.
  A cancellation that wins the lock, an expired/lost lease, or insufficient remaining budget
  returns no scheduled call.

Verify with `npm run factory:verify`. PostgreSQL concurrency coverage runs when the documented
`DATABASE_URL` integration environment is available; deterministic in-memory concurrency and
fencing tests always run.
