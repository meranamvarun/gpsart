import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { renderMonitoring } from "../scripts/render-monitoring.mjs";
import { evaluateControlledSignals, validateMonitoringBundle } from "../scripts/verify-monitoring.mjs";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const json = async (path) => JSON.parse(await read(path));

test("production dashboard covers every operational signal with bounded labels", async () => {
  const dashboard = await json("ops/observability/dashboard.json");
  const required = new Set([
    "api_health", "acknowledgement_latency_ms", "queue_delay_ms", "queue_backlog",
    "stuck_leases", "worker_throughput", "search_latency_ms", "terminal_outcome_total",
    "provider_failures_total", "timeout_total", "cancellation_total", "cleanup_failures_total",
  ]);
  const actual = new Set(dashboard.panels.flatMap((panel) => panel.metrics));
  for (const metric of required) assert.ok(actual.has(metric), `missing dashboard metric ${metric}`);
  const unsafeGrouping = /by\s*\([^)]*(?:correlation|job_id|attempt|provider_call|coordinate|latitude|longitude|geometry|trace|custom|user|session)/i;
  for (const panel of dashboard.panels) assert.doesNotMatch(panel.query, unsafeGrouping);
  for (const panel of dashboard.panels) {
    assert.ok(panel.id > 0 && panel.type && panel.gridPos);
    assert.ok(panel.targets.length > 0);
    for (const target of panel.targets) assert.doesNotMatch(target.expr, /;/);
  }
  const unsafe = /correlation|job|attempt|provider_call|coordinate|latitude|longitude|geometry|trace|custom|user|session/i;
  for (const dimension of dashboard.allowed_dimensions) assert.doesNotMatch(dimension, unsafe);
});

test("every alert is owned, routed, runnable, and passes controlled signal tests", async () => {
  const [alerts, signals, ownership, runbook] = await Promise.all([
    json("ops/observability/alerts.json"),
    json("ops/observability/controlled-signals.json"),
    json("ops/observability/ownership.json"),
    read("docs/runbooks/monitoring-incident-response.md"),
  ]);
  const report = validateMonitoringBundle({ alerts, signals, ownership, runbook });
  assert.deepEqual(report.errors, []);
  assert.deepEqual(evaluateControlledSignals(alerts, signals).failures, []);
  for (const heading of ["Triage", "Escalation", "Mitigation", "Closure"]) {
    assert.match(runbook, new RegExp(`## ${heading}`));
  }
  const byId = new Map(alerts.rules.map((rule) => [rule.id, rule]));
  assert.equal(byId.get("monitoring_watchdog")?.query, "vector(1) == 1");
  assert.equal(byId.get("monitoring_watchdog")?.route, "gpsart-deadman");
  assert.match(
    byId.get("worker_stalled")?.query ?? "",
    /or\s+vector\(0\)/,
    "a fresh worker with backlog and no terminal-outcome series must still alert",
  );
  assert.doesNotMatch(
    byId.get("alert_delivery_failure")?.query ?? "",
    /receiver\s*=/,
    "notification-failure detection must not depend on Alertmanager's optional receiver-name metric label",
  );
});

test("privacy audit covers every production telemetry and support boundary", async () => {
  const audit = await json("ops/observability/privacy-boundaries.json");
  assert.deepEqual(audit.boundaries.map(({ id }) => id).sort(), [
    "application", "database_audit", "edge", "error_reporting", "monitoring", "support",
  ]);
  const forbidden = new Set(audit.forbidden_fields);
  for (const field of [
    "query_strings", "request_bodies", "cookies", "authorization_headers", "csrf_headers",
    "exact_coordinates", "full_traces", "custom_text", "credentials", "tokens",
  ]) assert.ok(forbidden.has(field), `privacy audit omits ${field}`);
  for (const boundary of audit.boundaries) {
    assert.equal(boundary.reviewed, true);
    assert.ok(boundary.controls.length > 0);
    assert.ok(boundary.evidence.length > 0);
  }
});

test("deployment imports monitoring exports and fails closed on missing routing configuration", async () => {
  const [workflow, renderer, template, config, dashboard, alerts, collector] = await Promise.all([
    read(".github/workflows/deliver.yml"),
    read("scripts/render-monitoring.mjs"),
    read("deploy/kubernetes/monitoring.yaml.tmpl"),
    json("deploy/environments/staging.json"),
    json("ops/observability/dashboard.json"),
    json("ops/observability/alerts.json"),
    json("ops/observability/collector-contract.json"),
  ]);
  assert.match(workflow, /render-monitoring\.mjs/g);
  assert.match(workflow, /gpsart\.dev\/release-phase=monitoring/g);
  assert.match(workflow, /verify-monitoring\.mjs/g);
  assert.match(renderer, /alertReceiverSecretRef/);
  assert.match(renderer, /monitoringOwner/);
  assert.match(template, /kind: PrometheusRule/);
  assert.match(template, /kind: AlertmanagerConfig/);
  assert.match(template, /grafana_dashboard/);
  assert.doesNotMatch(template, /https?:\/\/[^_\s]*@|token:|password:/i);
  const rendered = renderMonitoring(
    template,
    config,
    { sourceRevision: "a".repeat(40) },
    dashboard,
    alerts,
    collector,
  );
  assert.doesNotMatch(rendered, /__[A-Z_]+__/);
  assert.match(rendered, /gpsart-collector-contract/);
  assert.throws(
    () => renderMonitoring(template, { ...config, alertReceiverSecretRef: "" }, { sourceRevision: "a".repeat(40) }, dashboard, alerts, collector),
    /alertReceiverSecretRef/,
  );
});

test("correlation remains opaque and spans browser acknowledgement through terminal work", async () => {
  const [api, client, worker, persistence, observability] = await Promise.all([
    read("crates/gpsart-api/src/lib.rs"),
    read("src/client/planner-workflow.ts"),
    read("crates/gpsart-worker/src/production.rs"),
    read("crates/gpsart-persistence/src/jobs.rs"),
    read("crates/gpsart-observability/src/lib.rs"),
  ]);
  assert.match(api, /x-correlation-id/);
  assert.match(client, /x-correlation-id/i);
  assert.match(worker, /Correlation::for_job\(lease\.job_id\)/);
  assert.match(persistence, /Correlation::for_job/);
  assert.match(observability, /correlation_id: Uuid/);
  assert.doesNotMatch(observability, /correlation_id:\s*(?:String|.*point|.*session)/i);
});
