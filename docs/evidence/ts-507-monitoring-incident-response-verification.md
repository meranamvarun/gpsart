# TS-507 monitoring and incident-response verification

Verified by the development owner on 2026-10-19. This record covers the checked-in monitoring exports and the deployment import path; it does not claim access to a live production Grafana or Alertmanager user interface.

## Acceptance evidence

1. `ops/observability/dashboard.json` is a Grafana dashboard export covering API health, acknowledgement p95, queue delay/backlog, stuck leases, worker throughput, route-search and end-to-end latency, truthful outcomes, provider failures, timeouts, cancellations, and cleanup. Queries group only by the closed low-cardinality dimensions from the TS-402 contract.
2. `ops/observability/alerts.json` defines twelve Prometheus alerts. Every rule has a severity, named owner, Alertmanager route, duration, controlled condition, and runbook anchor. `controlled-signals.json` exercises a healthy baseline and every rule; `node scripts/verify-monitoring.mjs` passed all 12 scenarios.
3. `privacy-boundaries.json` records the edge, application, database-audit, monitoring, error-reporting, and support review. It bans query strings, request bodies, cookies, authorization/CSRF headers, exact coordinates, full traces, custom text, credentials, and tokens. Edge and ingress access controls plus disabled PostgreSQL statement/connection logging are deployment-enforced.
4. The browser rejects an accepted route-job response unless `x-correlation-id` equals the opaque job UUID. API acknowledgement, persistence/attempt, worker/search/provider, and terminal metrics retain that UUID; attempt and provider-call refinements are bounded numeric fields when present, never a person or location.
5. Monitoring-pipeline absence, alert-delivery failure, cleanup failure, and stale worker/lease conditions have explicit alerts and procedures covering triage, escalation, mitigation, and closure. Primary and independent deadman receivers are secret references; receiver values are not stored in source or rendered evidence.
6. `scripts/render-monitoring.mjs` renders the dashboard, collector contract, PrometheusRule, and AlertmanagerConfig into the environment namespace. The staging and production delivery jobs verify the controlled signals, apply only the `monitoring` release phase, and record the imported custom resources.

## Test record

- Initial focused test: failed because `scripts/verify-monitoring.mjs` did not yet exist, establishing the red test before implementation.
- `node --test tests/monitoring-operations-contract.test.mjs tests/observability-contract.test.mjs tests/privacy-contract.test.mjs`: 10 passed.
- `npm exec -- tsx --test tests/planner-workflow.test.ts`: 15 passed, including mismatched-correlation rejection.
- `npm run typecheck`: passed.
- `DATABASE_URL=postgres://postgres@127.0.0.1:55432/postgres npm run factory:verify`: passed in 88.7 seconds with the mandatory connected PostgreSQL/PostGIS test executed.

The first unconfigured full-gate attempt failed only because the mandatory connected test correctly rejected an absent `DATABASE_URL`. The repository's disposable PostGIS instance was checked with `pg_isready`; the focused connected test and the complete factory gate then passed with the documented test database configured.

## Independent QA

Independent QA on 2026-10-19 inspected commit `e205239`, parsed the rendered production YAML and embedded dashboard/collector JSON, and confirmed all five acceptance criteria against the implementation. QA found and fixed INC-068: controlled conditions had hidden three production PromQL gaps. The stale-worker rule now fires when a fresh deployment has no terminal series, notification failures no longer depend on an optional receiver label, and the independent deadman route receives a continuous watchdog so complete monitoring-system silence is observable. Focused monitoring/privacy tests, planner correlation tests, typecheck, and the complete database-backed factory gate pass after the correction.
