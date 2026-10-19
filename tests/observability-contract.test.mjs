import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const readJson = async (path) => JSON.parse(await readFile(new URL(path, import.meta.url), "utf8"));

test("dashboard covers the release metrics with coarse dimensions", async () => {
  const dashboard = await readJson("../ops/observability/dashboard.json");
  const metrics = new Set(dashboard.panels.flatMap((panel) => panel.metrics));
  for (const required of [
    "acknowledgement_latency_ms", "queue_delay_ms", "graph_loading_latency_ms",
    "search_latency_ms", "provider_latency_ms", "validation_latency_ms",
    "snapshot_persistence_latency_ms", "total_latency_ms", "terminal_outcome_total",
    "cancellation_total", "timeout_total", "provider_calls_total",
    "candidates_returned_total",
  ]) assert.ok(metrics.has(required), `missing dashboard metric ${required}`);
  assert.deepEqual(dashboard.allowed_dimensions.sort(), [
    "activity", "algorithm_version", "design_category", "distance_bucket",
    "map_data_version", "profile_version", "result_class", "template_version",
  ]);
});

test("alert rules cover every required operational failure", async () => {
  const alerts = await readJson("../ops/observability/alerts.json");
  assert.deepEqual(alerts.rules.map((rule) => rule.id).sort(), [
    "cleanup_failure", "error_rate", "provider_failure_rate", "queue_backlog",
    "stuck_leases", "timeout_rate",
  ]);
  for (const rule of alerts.rules) {
    assert.ok(rule.query.length > 10);
    assert.match(rule.runbook, /^docs\/runbooks\//);
    assert.ok(rule.for_seconds > 0);
  }
});

test("API, queue, fitting, provider and terminal paths emit the shared contract", async () => {
  const checks = [
    ["../crates/gpsart-api/src/lib.rs", "AcknowledgementLatency"],
    ["../crates/gpsart-persistence/src/jobs.rs", "QueueDelay"],
    ["../crates/gpsart-worker/src/production.rs", "GraphLoadingLatency"],
    ["../crates/gpsart-routing/src/heart.rs", "SearchLatency"],
    ["../crates/gpsart-worker/src/lib.rs", "ProviderCalls"],
    ["../crates/gpsart-worker/src/production.rs", "ValidationLatency"],
    ["../crates/gpsart-worker/src/production.rs", "SnapshotPersistenceLatency"],
    ["../crates/gpsart-persistence/src/jobs.rs", "TerminalOutcome"],
  ];
  for (const [path, marker] of checks) {
    assert.match(await readFile(new URL(path, import.meta.url), "utf8"), new RegExp(`MetricName::${marker}`));
  }
});
