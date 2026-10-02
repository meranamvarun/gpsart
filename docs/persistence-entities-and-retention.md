# Persistence entities, ownership, and retention

TS-002 establishes the durable PostgreSQL/PostGIS boundary. Identifiers are UUIDs, instants are
`timestamptz` values (stored as UTC instants), distances are canonical integer metres, and database
rows never contain plaintext guest credentials. `token_hash` is the only persisted cookie-derived
value.

```text
guest_sessions
  |-- resolved_starts
  |     `-- planning_requests (immutable snapshot)
  |             `-- generation_jobs
  |                   |-- generation_job_outcomes (zero while active, exactly one when terminal)
  |                   |-- job_attempts
  |                   `-- route_candidates (immutable version)
  `-- cleanup_run_failures ---- cleanup_runs
```

Every private child carries or is reached through `guest_session_id`. Composite foreign keys enforce
that a resolved start, planning request, job, and candidate cannot be connected across owners.
`(guest_session_id, idempotency_key)` is unique for immutable request snapshots. A later service must
compare the stored `input_hash` before treating a repeated key as the same request.

## Spatial representation and GeoJSON

Requested pins, confirmed starts, candidate endpoints, and canonical route geometry use SRID 4326.
Points use `geometry(Point, 4326)` and full canonical traces use
`geometry(LineString, 4326)`. Coordinates remain longitude, latitude at the GeoJSON boundary.

Convert for output with `ST_AsGeoJSON(canonical_route)::jsonb`. Convert validated GeoJSON input with
`ST_SetSRID(ST_GeomFromGeoJSON($1), 4326)` and then require the table's typed geometry column to accept
it. Conversion is representation-only: it must never simplify, snap, infer, or fabricate geometry.

## Immutable snapshots and outcomes

`planning_requests` captures the requested distance and tolerance in metres plus the complete
route-affecting profile JSON, profile version, template ID/version, map-data version, algorithm
version, requested pin, revision, and SHA-256 input hash. Updates are rejected. Route candidates are
append-only versions per job; updates are rejected. Retention cleanup may delete the owning guest
session and relies on cascading deletion.

The constrained job state is one of `queued`, `running`, `succeeded`, `no_match`, `failed`,
`cancelled`, or `timed_out`. Deferred database triggers require no terminal outcome for active jobs
and exactly one matching `generation_job_outcomes` row for terminal jobs, allowing the state and
outcome to be written atomically in either order inside one transaction.

## Retention and cleanup metadata

The proposed default retention for unsaved guest work is **24 hours**. Callers set `expires_at` and
`cleanup_after`; TS-401 will implement policy and batching. `cleanup_runs` records cutoff, status,
counts, and retry timing. `cleanup_run_failures` records only an owner UUID and public error code; it
must not store exact coordinates, traces, credentials, provider payloads, or private exception text.
Deleting a guest session cascades through all precise child data. Backup deletion guarantees remain
deployment policy work for TS-401 and must not be inferred from this schema.

## Startup and deployment

Deployment invokes `gpsart_persistence::apply_migrations` explicitly. API and worker startup use
`PostgresProbe`, which compares every applied SQLx version and checksum with the build's embedded
migrations. Missing, pending, failed, changed, or unknown versions prevent the process from becoming
available; application startup never applies migrations implicitly.

