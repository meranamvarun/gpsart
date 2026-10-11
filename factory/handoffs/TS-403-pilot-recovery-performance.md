# TS-403 pilot recovery/performance handoff

- Version: `gpsart-pilot-load-v1` / `gpsart-pilot-report-v1` / `gpsart-failure-injection-v1`
- Producer: TS-403
- Consumer: TS-404 release verification
- Compatibility: workload inputs and provider/map/algorithm pins are immutable. New runs append or replace a report only with recorded hardware, exact command, and honest gate decisions.

Inputs live at `ops/performance/ts403-workload.json`. Measured output is `docs/evidence/ts-403-pilot-performance.json`; recovery evidence is `docs/evidence/ts-403-failure-injection.json`.

The release-significant finding is a failed 30-second p95 termination target (68.886 seconds) and failed 60-second hard maximum (75.507 seconds) under 20-way load on the recorded four-core machine. The acknowledgement target passed. These corrected independent-QA measurements include queue delay; TS-404 must carry both performance misses into its release decision and may not average them away.

Verification:

```text
node scripts/cargo.mjs test -p gpsart-routing --test pilot_load -- --ignored --nocapture
node scripts/cargo.mjs test -p gpsart-worker --test shutdown
node scripts/cargo.mjs test -p gpsart-persistence --test job_coordinator
node --test tests/pilot-hardening-contract.test.mjs
```
