# Pilot workload, privacy, and retention policy

This is the deployable first-release policy for unsaved guest work. Precise route records are temporary service data, not analytics data.

## Workload limits

The validated `WorkloadLimits` configuration fails startup when a deployment attempts to retain guest work for more than 24 hours, run a job longer than 60 seconds, or allow a per-guest concurrency cap above the deployment cap. Defaults are:

| Dimension | Default |
| --- | ---: |
| Requested distance | 1,000–100,000 metres |
| Distance tolerance choices | 5%, 10%, or 20% |
| Template points / ordered anchors | 512 / 64 |
| Candidate attempts / provider requests | 3 / 50 |
| JSON request / route response | 64 KiB / 2 MiB |
| Job runtime | 60 seconds |
| Active jobs per guest / deployment | 2 / 20 |
| New submissions per guest per minute | 10 |
| Unsaved guest retention | 24 hours |

PostgreSQL admission takes a deployment-wide advisory lock, checks idempotent replay first, then counts active and recent work. A new idempotency key therefore cannot evade the guest limit. Rejected work returns `429` with `Retry-After` and `retry_after_seconds`. Provider reservations increment a durable counter under the job row lock before the call is released for scheduling.

## Cleanup and deletion

The API process runs cleanup immediately at startup and every 30 seconds until graceful shutdown. Cleanup processes at most 100 expired request families per transaction. It deletes snapshots/candidates before jobs and immutable requests, then removes unreferenced expired starts and only then sessions with no remaining precise records. A request family protected by a non-expired worker lease is skipped. Each run records only counts and whether another batch is required; retry is scheduled after 30 seconds. Cleanup never materializes route geometry.

Unsaved requests and candidates receive an expiry no later than 24 hours after acceptance. Device-saved routes are browser-owned and unaffected by server cleanup.

Production database backups must be encrypted, access-audited, retained for no more than seven days, and deleted by the storage lifecycle policy. A restored backup must remain isolated until migrations run and expired guest data is cleaned. Thus deletion reaches the live database on the cleanup schedule and all backup copies within seven days. A legal hold or recovery exception must be recorded as a release/privacy incident; it must never silently extend normal retention.

## Logging and analytics boundary

Application, worker, persistence, reverse-proxy, error-reporting, and analytics sinks must not record:

- exact requested or confirmed coordinates;
- canonical geometry, route snapshots, full traces, or provider payloads;
- Cookie, CSRF, guest-token, database URL, or provider-credential values;
- current or future custom text.

Reverse-proxy access logs disable query strings, request/response bodies, and Cookie/authorization headers. Error reporting receives public error codes and opaque correlation IDs only. Analytics receives coarse categories and buckets only. The CI privacy audit examines every structured `tracing` event for protected field names, while `SecretString` keeps credentials redacted under `Debug`.

## Provider and public-map credentials

The selected pilot routing provider is the server-side pinned local OSM graph and requires no browser credential. Any future provider credential must enter only the API/worker environment as a redacted secret and must never be serialized to clients. The current public OpenStreetMap tile endpoint uses no key. If a keyed public map service is introduced, the key must be separately classified as public, restricted to the production origins at the provider, scoped to tile reads, quota-capped, and rotated independently of server credentials.
