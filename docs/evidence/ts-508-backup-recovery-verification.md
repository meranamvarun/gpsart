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

## Independent QA review

Independent QA reviewed `9e30235..092c629` and the subsequent QA corrections on 2026-10-19. The review found and fixed INC-069 through INC-073: a pipeline that could hide `pg_dump` failure, a false-success audit that omitted missing outcome rows, rollback compatibility not bound to an exact reviewed revision, missing writable-volume group ownership in the recovery PostgreSQL pod, and incomplete backup success/age monitoring. The corrected audit validates complete `CandidateSnapshot` values rather than only checking the geometry type.

Recovery and monitoring contracts passed 11/11; deployment contracts passed 8/8. The three distinct database-backed commands referenced by the six-scenario failure-injection matrix passed, including truthful persistence failure, owner-scoped durable status, and stale-lease fencing. The final database-backed `npm run factory:verify` passed in 98.3 seconds, including strict Clippy, 19 PostgreSQL migration tests, worker tests, client checks, production build, and factory contracts. PyYAML 6.0.3 parsed all three changed workflows plus the four-document production recovery render and eight-document isolated rehearsal render.

QA does not approve TS-508 as complete. The local commands do not constitute a real database interruption, API restart, worker termination, or network partition in the protected environment. No successful encrypted production-repository backup/access-audit record, isolated Kubernetes restore transcript, compatible and unsafe live rollback transcripts, production RPO/RTO measurement, data-deletion approval, operations review, or accountable owner approvals were supplied. The story therefore remains `in_review` and must not be marked `done` or treated as completed release evidence.
