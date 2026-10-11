# TS-402 privacy-preserving observability verification

Verified 2026-10-11.

## Acceptance evidence

1. Accepted jobs return the opaque job UUID as `x-correlation-id`; API acknowledgement, queue/attempt, observed fitting, provider, candidate count, and terminal events share that UUID.
2. The shared typed contract exposes acknowledgement, queue, search, provider and total latency; outcomes, cancellation, timeout, provider calls, candidates, backlog, stuck leases, errors, and cleanup failures.
3. Route dimensions permit only activity, coarse distance bucket, shape/text category, controlled versions, and a closed set of machine result classes. Coordinate-like, free-form, and unknown result labels are replaced before reaching tracing, and validated fields cannot be mutated after construction.
4. `ops/observability/dashboard.json` contains release percentile/distribution queries. `ops/observability/alerts.json` covers backlog, stuck leases, provider failures, errors, timeouts, and cleanup failures with runbook links.
5. Privacy tests inspect every structured event in the instrumented boundaries. Rust tests prove representative coordinate payloads are removed before serialization to the logging sink.

## Results

- `gpsart-observability`: 5 passed.
- API route-job, worker/provider, heart fitting, and curated-text focused suites: 17 passed.
- Persistence migration suite: 16 passed; terminal metric-family regression: 1 passed.
- Observability and privacy contract tests: 5 passed.
- Workspace strict Clippy: passed.
- `npm run factory:verify`: passed, including all Rust/TypeScript/factory/visual/Sites tests and production build.

Independent QA fixed and recorded `INC-045` (coordinate/high-cardinality result labels bypassed redaction) and `INC-046` (automatic timeout/attempt exhaustion and cancellation omitted parts of the terminal metric family) before approval.
