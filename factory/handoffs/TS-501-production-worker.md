# TS-501 production worker contract

- Version: `production-worker-v1`
- Producer: TS-501
- Consumers: TS-502, TS-505, TS-508
- Compatibility: additive persistence API; existing job submission/status and snapshot readers are unchanged

## Contract

The worker claims `generation_jobs` through `PostgresJobCoordinator`, then loads the immutable request only while the same `(job_id, worker_id, attempt)` lease is live. The loaded input includes the exact confirmed start, request revision, profile, coverage, template, routing-data and algorithm versions, distance bounds, finish preference, deadline, owner and expiry.

Production startup requires `GPSART_ROUTING_GRAPH_PATH` and `GPSART_ROUTING_GRAPH_SHA256_PATH`. `LocalOsmGraph` verifies the external SHA-256 and exposes its canonical `<snapshot-id>:sha256:<digest>` data version. A request whose map-data pin differs is rejected before routing.

Heart and curated-text requests run through `CandidateSearch` and the checksum-pinned local OSM provider. Each provider call first reserves the durable provider-call budget. Only `ScoredCandidate` values that pass legality and quality validation can become `CandidateSnapshot` values.

`PostgresJobCoordinator::publish_success` is the publication boundary. It rechecks the live lease and deadline, validates/inserts the immutable snapshot, records the attempt, transitions the job, and inserts the terminal outcome in one transaction. Consumers may therefore treat a succeeded result ID as atomically readable; an orphan candidate cannot be exposed by this path.

The worker renews its lease during search. Cancellation, lease loss, deadline loss, or database loss cancels further work and prevents publication. Graceful process shutdown drops in-flight work; the durable lease later permits bounded recovery without duplicate publication.

## Verification

```text
npm run test:factory
node scripts/cargo.mjs test -p gpsart-worker --all-targets
node scripts/cargo.mjs test -p gpsart-persistence --test migrations
node scripts/cargo.mjs clippy -p gpsart-worker -p gpsart-persistence --all-targets -- -D warnings
```

PostgreSQL integration requires `DATABASE_URL` to name an administrator role able to create isolated databases. The tests deliberately fail rather than downgrade to a shared schema when that role lacks `CREATEDB`.

## Example

Given a queued heart request pinned to `bengaluru-central-pilot-v1`, an exact E7 confirmed start,
and the map-data version produced by the configured graph checksum, the worker loads that immutable
request revision, reserves each provider call durably, and publishes a validated snapshot plus the
terminal success outcome in one lease-fenced transaction. If the stored coverage or map version is
unknown, the start is not exactly representable, the lease is lost, or the database cannot reserve a
provider call, no candidate is published. The tracked non-secret runtime shape is
`ops/worker.env.example`; the database URL must come from the deployment secret store.
