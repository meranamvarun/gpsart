# TS-203 job-status handoff

Producer: TS-203
Consumers: TS-204, TS-302
Contract version: 1

- `GET /api/v1/route-jobs/{job_id}` authenticates with the private guest-session cookie and
  returns `404` for absent or foreign jobs. Read-only polling does not require Origin or CSRF
  headers.
- The response contains the immutable request revision, durable state and `status_version`, the
  last observed coarse stage, queued/started/completed/deadline/expiry epoch timestamps, a
  terminal outcome when present, published result IDs, and polling/retry guidance. It never
  returns an estimated percentage.
- Active clients poll after `poll_after_seconds` and send the returned strong `ETag` in
  `If-None-Match`. An unchanged representation returns `304` with no body. Terminal responses set
  `poll_after_seconds` to null.
- Stages are `queued`, `checking_roads`, `fitting_design`, and `comparing_routes`. Workers advance
  them durably and monotonically while holding the current fenced lease.
- Terminal public codes distinguish `succeeded`, `unsupported_coverage`, `no_routable_access`,
  `disconnected_network`, `insufficient_visual_match`, `outside_distance`, `exhausted_budget`,
  `provider_unavailable`, `attempts_exhausted`, `cancelled`, and `timed_out`. Only provider
  unavailability currently advises a retry (30 seconds). `legacy_unknown` is reserved for
  migrated historical rows and cannot be published by a worker.
- State, classification, completion timestamp, and any already-validated candidate references
  commit in one transaction. The status read returns candidate IDs only for a succeeded job and
  rejects inconsistent stored state instead of guessing.
- The TypeScript client boundary is `decodeRouteJobStatus` in `src/domain/job-status.ts`. It is
  intentionally strict about exact fields, version-4 UUIDs, lifecycle consistency, classifications,
  budget flags, and retry guidance.

Verify with `npm run factory:verify`. PostgreSQL migration/state-transition coverage executes when
`DATABASE_URL` is configured and otherwise reports an explicit skip; deterministic API, client,
and in-memory transition checks always run.
