# TS-507 monitoring and incident-response handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-603  
**Compatibility:** additive operational contract. TS-402 metric names and allowed route dimensions remain unchanged; alert and dashboard additions must stay privacy-safe and low-cardinality.

## Published contract

- The platform collector consumes JSON application stderr events where `event=operational_metric`, promotes only the metric value and the eight allow-listed coarse dimensions, and keeps correlation/job/attempt/provider-call fields out of metric labels.
- Grafana imports the ConfigMap labelled `grafana_dashboard=1`. Prometheus Operator imports `gpsart-production-alerts`; Alertmanager Operator imports `gpsart-alert-routing`.
- Application pages route to `gpsart-oncall`. Monitoring and notification-delivery pages use the independent `gpsart-deadman` route owned by `platform-oncall`; receiver URLs come only from namespace Secrets. A continuously firing `monitoring_watchdog` lets that external receiver detect a complete Prometheus or Alertmanager outage.
- The delivery workflow renders revision-pinned runbook URLs, verifies all controlled signal scenarios, applies the separate `monitoring` phase after runtime readiness, and includes PrometheusRule/AlertmanagerConfig state in the deployment record.
- Browser/API/job/attempt/worker/provider/terminal correlation uses the accepted job UUID. Exact locations, full traces, session identity, custom text, and credential material are not correlation data and are prohibited from every reviewed path.
- Operator response follows `docs/runbooks/monitoring-incident-response.md`. Unknown or failed routing work stays explicit; incident mitigation may not loosen route rules or manufacture a successful result.

## Verification

```text
node scripts/verify-monitoring.mjs
node --test tests/monitoring-operations-contract.test.mjs tests/observability-contract.test.mjs tests/privacy-contract.test.mjs
npm exec -- tsx --test tests/planner-workflow.test.ts
DATABASE_URL=<disposable PostGIS admin database> npm run factory:verify
```
