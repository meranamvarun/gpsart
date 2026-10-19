# TS-506 production performance handoff

- Version: `gpsart-production-performance-v1`
- Producer: `codex/root`
- Consumers: TS-601, TS-603
- Compatibility: additive observability and evidence contract; public API, routing, snapshot, and deployment thresholds are unchanged

## Contract

`npm run test:performance` is the release-only connected performance gate. It requires `DATABASE_URL` for an administrative PostgreSQL/PostGIS database, creates and drops an isolated migrated database, uses the production router and checksum-pinned graph, and runs 20 accepted jobs through two real `ProductionWorker` instances. The command fails when acknowledgement p95 exceeds one second, termination p95 exceeds 30 seconds, any job exceeds 60 seconds, overload lacks bounded retry guidance, or cancellation, cleanup, lease renewal, and terminal truthfulness are not preserved.

`WorkerJobProfile` exposes aggregate durations and renewal counts only. Operational metrics add `graph_loading_latency_ms`, `validation_latency_ms`, and `snapshot_persistence_latency_ms` to the existing queue, search, provider, and total timings. None of these values contains precise route input or geometry.

The reviewed workload is `ops/performance/ts506-workload.json`; the recorded output is `docs/evidence/ts-506-production-performance.json`. Measurements are valid only for the recorded release profile, hardware, immutable graph/algorithm pins, worker count, and command. A later run replaces the report only with its own truthful hardware and outcome record; thresholds cannot be weakened after measurement.

## Verification

```text
npm run test:performance
node --test tests/production-performance-contract.test.mjs
npm run factory:verify
```
