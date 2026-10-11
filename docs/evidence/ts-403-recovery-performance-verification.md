# TS-403 recovery and pilot performance verification

Verified 2026-10-11.

## Workload and measured result

The explicit load command ran 20 simultaneous jobs against the checksum-pinned Bengaluru local OSM snapshot on an AMD Ryzen 3 5300U (4 cores / 8 logical processors, 7.3 GiB RAM, Windows 11). The workload spans walking, running, cycling, four requested-distance values, shape and curated-text templates, cancellation, provider timeout, and provider quota injection.

The checked-in JSON report records p50/p95/p99 acknowledgement, queue, provider, and end-to-end total latency; result quality; no-match, timeout, cancellation, quota, and provider-call distributions. Independent QA found and recorded INC-047 because the implementation run had excluded queue delay from total latency. The corrected QA run passed acknowledgement p95 at 0.103 ms, but total p95 was 68.886 seconds and the maximum was 75.507 seconds. Both the 30-second target and immutable 60-second cap therefore missed. These misses are explicit release decisions and are not hidden by the 5.738-second median; timings vary with contention on the recorded machine.

The load test is intentionally ignored by the ordinary unit gate and run explicitly with `--ignored --nocapture`: it saturates the recorded machine and is evidence generation, not a deterministic unit assertion. Contract tests keep the committed workload and report shape in the normal factory gate.

## Recovery evidence

- Worker startup and mid-loop database loss now recheck readiness and resume without an external restart.
- Lease tests fence terminated/expired workers by worker UUID and attempt, reclaim only after expiry, and reject stale or duplicate publication.
- Provider timeout/quota injections return no candidate or false success; provider work cannot start without a durable permit.
- Existing route-presentation and journey tests prove a slow/absent base map changes only map status while immutable route facts, text traversal, selection, and route-affecting state remain available.

## Verification

- Explicit `pilot_load`: 20/20 completed after the end-to-end timing regression was fixed; report generated from actual pinned-provider calls.
- Worker shutdown/reconnect suite: 5 passed.
- Persistence lifecycle/recovery suite: 7 passed.
- Base-map route presentation/journey suite: 13 passed.
- Pilot hardening contracts: 3 passed.
- Workspace strict Clippy: passed.
- `npm run factory:verify`: passed.
