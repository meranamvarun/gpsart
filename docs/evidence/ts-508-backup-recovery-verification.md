# TS-508 backup, recovery, and rollback verification

## Acceptance checks implemented

1. A digest-pinned recovery image streams PostgreSQL custom dumps to authenticated-encrypted restic storage, prunes to at most seven days, requires an access-audited repository, and adds privacy-safe stale/failure alerts.
2. The recovery workflow creates a run-scoped, default-deny namespace with no public traffic resource. It restores, applies forward migrations, verifies embedded checksums, runs bounded expiry cleanup, parses immutable snapshots, checks owner consistency, and rejects false success.
3. `ops/recovery/failure-injection.json` maps all six required interruption classes to deterministic persistence/worker/API checks and requires no duplicate publication, corrupt snapshot, or false success.
4. Rollback now queries the live successful schema version and fails before applying an older runtime unless the target compatibility window explicitly includes that schema.
5. The disaster-recovery runbook names database, operations, privacy, and incident ownership, measurement objectives, evidence boundaries, deletion checks, stop conditions, and closure requirements.

## Development database rehearsal

On 2026-10-19, a fresh PostgreSQL 18.6/PostGIS development database was migrated, seeded only with one deterministic expired session, dumped in custom format, restored into a second isolated database, migrated, and audited. The dump took 633 ms. Restore, migration, cleanup, and audit took 8.182 s. The audit reported current migration checksums, zero ownership mismatches, zero expired private rows, zero false successes, and zero snapshots in the intentionally minimal fixture. Both temporary databases were dropped after the audit. The dump contains no real locations or user data.

This result proves the local database mechanics and the privacy-safe audit output. It is not a production RPO/RTO measurement, encrypted repository test, storage audit, Kubernetes network-isolation test, rollback execution, or accountable-owner approval. `ops/recovery/local-rehearsal-evidence.json` keeps those limitations machine-readable and deliberately contains no approvals.

## External completion evidence still required

Before TS-508 can truthfully become `done`, the protected disaster-recovery environment must produce:

- a successful encrypted backup record plus access-audit and seven-day lifecycle confirmation;
- a sanitized isolated restore record with migration, snapshot, ownership, and cleanup checks;
- executed failure-injection and compatible/unsafe rollback transcripts;
- production RPO/RTO measurements; and
- explicit database-owner, operations-owner, privacy-owner, and independent QA decisions.

The workflow uploads only sanitized audit and timing artifacts. Exact coordinates, geometry, request bodies, cookies, credentials, database URLs, object names, and dump contents are excluded.
