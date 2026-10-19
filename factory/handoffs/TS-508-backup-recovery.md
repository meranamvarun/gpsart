# TS-508 backup, recovery, and rollback handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-603  
**Compatibility:** additive fifth `recovery` image in the immutable artifact manifest; migrations remain forward-only and runtime rollback is allowed only inside the checked schema window.

## Published contract

- Production installs a digest-pinned recovery image and a six-hour encrypted backup CronJob. Repository and database credentials are separate managed-secret records. Retention is independently bounded to seven days by restic pruning and the required storage lifecycle.
- `scripts/verify-recovery.mjs` fails closed on missing encryption, storage auditing, objectives, owners, retention over seven days, unknown rollback targets, or schema versions outside the reviewed compatibility window.
- `.github/workflows/recovery-rehearsal.yml` restores into an exact run-scoped namespace with no public service. Migration checksums, expiry cleanup, snapshot readability, ownership consistency, and truthful success linkage must pass before any traffic could be admitted.
- `gpsart-recovery-audit` prints counts and state only. It does not select or log coordinates, route geometry, credentials, tokens, custom text, or object contents.
- The production rollback workflow checks the live migration version before applying only the historical runtime phase. An unsafe schema requires stopped traffic and a reviewed forward repair.

## Verification

```text
node --test tests/recovery-contract.test.mjs
node scripts/verify-monitoring.mjs
node scripts/cargo.mjs test -p gpsart-persistence --test migrations -- --test-threads=1
node scripts/cargo.mjs test -p gpsart-worker --test production_pipeline -- --test-threads=1
npm run factory:verify
```

Protected-environment backup storage, restore, RPO/RTO, and owner decisions must be attached to the release record. Source contracts and development-database results must never be represented as a production rehearsal.
