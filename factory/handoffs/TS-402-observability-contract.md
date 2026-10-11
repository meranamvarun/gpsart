# TS-402 observability contract handoff

- Version: `operational_metric` v1
- Producer: TS-402 / `gpsart-observability`
- Consumers: API, persistence queue, routing candidate search, worker/provider adapter, TS-403 load and recovery harnesses, deployment dashboards and alerts
- Compatibility: metric names and allowed dimensions are additive-only within v1. Exact values that can identify a route or guest are never compatible additions.

The generation-job UUID is the opaque correlation ID. `x-correlation-id` returns it on accepted job creation. Attempt and provider-call ordinals may refine that correlation but never replace it with a guest, coordinate, or request body value.

Example event dimensions are `walking`, `1_to_5km`, `shape`, and controlled profile/template/map/algorithm versions. See `ops/observability/dashboard.json` for queries and `ops/observability/alerts.json` for alert inputs.

Verification:

```text
node --test tests/observability-contract.test.mjs tests/privacy-contract.test.mjs
node scripts/cargo.mjs test -p gpsart-observability
npm run factory:verify
```
