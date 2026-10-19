# TS-506 production performance verification

The production-equivalent release run connects the production API router, an isolated migrated PostgreSQL/PostGIS database, two `ProductionWorker` instances matching the deployed replica count, the checksum-pinned routing graph, and durable candidate publication. Its 20 accepted jobs cover the reviewed shape/text catalogue, walking/running profiles, and three distance targets. Cycling is intentionally absent because the approved production manifest marks it unreviewed.

The measured run passed the immutable gates without changing them: acknowledgement p95 was 5.4212 ms (target at most 1 second), end-to-end termination p95 was 6.462426 seconds (target at most 30 seconds), and the maximum was 6.545399 seconds (cap 60 seconds). All 20 jobs reached truthful terminal states: 10 succeeded, 9 no-match, and the explicitly cancelled job remained cancelled. Existing sealed-candidate validation, fixed-start, access/turn, distance, version, snapshot, and GPX tests remain the legality regression suite; TS-506 does not change the routing algorithm or those contracts.

Profiling now names graph loading, candidate search, provider work, validation, snapshot persistence, queue, acknowledgement, and total latency using opaque job correlation and coarse bounded dimensions. The checked-in report contains aggregates only. It contains no coordinates, geometry, session material, credentials, or personal data.

At saturation, both production worker slots were active and the queue reached 20. A 21st valid job received HTTP 429 with the machine-readable `rate_limited` code and `Retry-After: 60` in 0.9308 ms. Cancellation completed while the queue was saturated, cleanup completed in 14.9584 ms, and a long-running search recorded a successful lease renewal.

Verification commands:

```text
npm run test:performance
node --test tests/production-performance-contract.test.mjs
node scripts/cargo.mjs test -p gpsart-observability --test observability
node scripts/cargo.mjs test -p gpsart-worker --all-targets -- --test-threads=1
npm run factory:verify
```

Independent QA must rerun the focused contract, inspect the aggregate report and diff, execute the invariant suites and `factory:verify`, and record its result in `factory/stories.json`. The implementation run is not an independent QA/SRE signature.
