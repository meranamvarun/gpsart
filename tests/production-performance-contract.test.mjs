import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");
const json = async (path) => JSON.parse(await source(path));

test("TS-506 workload is a pinned 20-job production-worker run", async () => {
  const workload = await json("../ops/performance/ts506-workload.json");
  assert.equal(workload.schema_version, "gpsart-production-load-v1");
  assert.equal(workload.concurrent_jobs, 20);
  assert.equal(workload.production_worker_replicas, 2);
  assert.equal(workload.cases.length, 20);
  assert.deepEqual([...new Set(workload.cases.map(({ category }) => category))].sort(), ["shape", "text"]);
  assert.deepEqual([...new Set(workload.cases.map(({ activity }) => activity))].sort(), ["running", "walking"]);
  assert.ok(new Set(workload.cases.map(({ requested_distance_metres }) => requested_distance_metres)).size >= 3);
  assert.ok(workload.cases.every(({ requested_distance_metres }) => Number.isInteger(requested_distance_metres)));
});

test("TS-506 report proves immutable latency gates and overload behavior", async () => {
  const report = await json("../docs/evidence/ts-506-production-performance.json");
  assert.equal(report.schema_version, "gpsart-production-performance-v1");
  assert.equal(report.sample_count, 20);
  assert.equal(report.build_profile, "release");
  assert.equal(report.connected_components.api, "production router");
  assert.equal(report.connected_components.queue, "PostgreSQL/PostGIS");
  assert.equal(report.connected_components.worker, "ProductionWorker");
  for (const metric of ["acknowledgement", "queue", "graph_loading", "candidate_search", "provider", "validation", "snapshot_persistence", "end_to_end"]) {
    const values = report.latency_milliseconds[metric];
    assert.ok(values && values.p50 <= values.p95 && values.p95 <= values.p99, `invalid ${metric} percentiles`);
  }
  assert.ok(report.latency_milliseconds.acknowledgement.p95 <= 1000);
  assert.ok(report.latency_milliseconds.end_to_end.p95 <= 30000);
  assert.ok(report.latency_milliseconds.end_to_end.maximum <= 60000);
  assert.equal(Object.values(report.terminal_outcomes).reduce((sum, value) => sum + value, 0), 20);
  assert.equal(report.overload.http_status, 429);
  assert.ok(report.overload.retry_after_seconds > 0);
  assert.equal(report.overload.body_code, "rate_limited");
  assert.equal(report.fairness.cancellation_completed_while_saturated, true);
  assert.equal(report.fairness.cleanup_completed_while_saturated, true);
  assert.equal(report.fairness.lease_renewal_stayed_live, true);
  assert.ok(report.resource_saturation.peak_active_worker_slots <= report.resource_saturation.worker_slots);
  assert.ok(report.resource_saturation.peak_queue_depth <= 20);
  assert.ok(report.gates.every(({ passed }) => passed));
});

test("TS-506 harness is release-only, connected, privacy-safe, and fail-closed", async () => {
  const [packageJson, harness, verification] = await Promise.all([
    json("../package.json"),
    source("../crates/gpsart-worker/tests/production_load.rs"),
    source("../docs/evidence/ts-506-production-performance-verification.md"),
  ]);
  assert.match(packageJson.scripts["test:performance"], /--release/);
  assert.match(packageJson.scripts["test:performance"], /production_load/);
  for (const marker of ["DATABASE_URL", "ProductionWorker", "MIGRATOR", "StatusCode::TOO_MANY_REQUESTS", "header::RETRY_AFTER", "cleanup_expired_sessions", "lease_renewals"]) {
    assert.match(harness, new RegExp(marker));
  }
  assert.doesNotMatch(harness, /TS506_REPORT=.*longitude|TS506_REPORT=.*latitude|TS506_REPORT=.*geometry/);
  assert.match(verification, /independent QA/i);
});
