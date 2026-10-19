# Disaster recovery and rollback

## Owners and objectives

- The database owner owns backup credentials, storage lifecycle, restore correctness, and the six-hour maximum RPO.
- The operations owner owns the isolated rehearsal, the one-hour maximum RTO, monitoring, traffic admission, and rollback execution.
- The privacy owner approves restored-data deletion and any retention exception.
- The incident commander may stop promotion or traffic. Only the database owner may declare a restored database fit for traffic.

The executable production values are in `ops/recovery/production.json`. A recovery exception, missed objective, failed audit, or retention extension is an incident and blocks release.

## Automated encrypted backup

The production delivery installs `gpsart-backup` from the same immutable artifact manifest as the application. Every six hours it streams a custom-format PostgreSQL dump into a restic repository. Restic provides authenticated encryption before bytes leave the pod. The repository credential comes from the managed secret store, and the storage account must have provider access auditing and lifecycle deletion enabled. The job additionally runs `forget --keep-within 7d --prune`; storage lifecycle is the independent upper-bound control.

`backup_stale` pages when no completion is observed within seven hours. `backup_failed` pages on a failed job. These rules use only CronJob/job status and never database output, object names, request fields, coordinates, or route data.

Before production use, the database owner records the storage account identifier as a non-secret opaque reference, confirms encryption and audit logging, tests a denied read, and confirms objects older than seven days are absent. Never attach object contents, credentials, database URLs, or dump logs.

## Isolated restore rehearsal

1. Dispatch `recovery-rehearsal.yml` with an approved delivery run. The workflow uses its digest-pinned recovery image and creates exactly `gpsart-recovery-<run-id>`.
2. Confirm the rendered resources contain no Ingress or LoadBalancer. Default-deny networking admits only namespace-local PostgreSQL, DNS, and repository HTTPS. No application or public edge is deployed.
3. The job restores the latest encrypted snapshot, runs all forward migrations, verifies embedded migration checksums, executes bounded expiry cleanup, parses every immutable snapshot, checks owner relationships, and rejects any success without a readable result.
4. A failure keeps traffic unadmitted and fails the workflow. Record sanitized job status, audit summary counts, snapshot creation/completion times, measured RPO/RTO, source revision, and image digest.
5. The exact recovery namespace is deleted after evidence upload. Confirm its PVC/empty storage is gone. Do not delete production data or the backup repository.

The database, operations, and privacy owners review the sanitized record. Their explicit decisions and timestamps belong in the protected release record; absence of any decision blocks release.

## Failure injection

Run every command in `ops/recovery/failure-injection.json`. Together they exercise database/persistence interruption, API process restart with durable state, worker loss and stale fencing, partial candidate persistence, and the database boundary used for network-partition behavior. For every scenario verify one terminal outcome at most, no duplicate candidate version, no exposed candidate without a complete immutable snapshot, and no success whose result is unreadable. Do not weaken deadlines or edit leases to make recovery pass.

## Rollback compatibility

Migrations are forward-only. Before applying an older runtime, `.github/workflows/rollback.yml` reads the current successful schema version and passes it with the selected manifest's exact source revision to `scripts/verify-recovery.mjs rollback`. Each exact revision in the closed compatibility window in `ops/recovery/rollback-compatibility.json` is reviewed with every migration that changes a binary/database contract; a generic alias such as `previous` is forbidden.

If the schema is outside the target window, stop rollback, keep traffic stopped, and execute a reviewed forward repair. Never run an old migrator, reverse a migration, or guess compatibility. After an allowed rollback, repeat readiness, the connected smoke, snapshot reads, and owner-isolation checks before traffic is restored.

## Evidence and closure

The protected recovery record must include sanitized backup/restore logs, every failure-injection result, the rollback transcript, measured RPO and RTO, expiry-cleanup counts, source and artifact digests, and three owner approvals. Logs are limited to timestamps, durations, versions, counts, Kubernetes reason/status values, and opaque run identifiers. Exact coordinates, geometry, request bodies, cookies, tokens, credentials, database URLs, object names, and user-provided text are forbidden.

Close a recovery incident only after a new encrypted backup succeeds, age/failure monitoring resolves, an isolated restore passes, overdue restored data is absent, and accountable owners sign the outcome.
