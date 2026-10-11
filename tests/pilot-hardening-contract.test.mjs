import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const json = async (path) => JSON.parse(await readFile(new URL(path, import.meta.url), "utf8"));

test("pilot workload has 20 concurrent representative immutable cases", async () => {
  const config = await json("../ops/performance/ts403-workload.json");
  assert.equal(config.schema_version, "gpsart-pilot-load-v1");
  assert.equal(config.concurrency, 20);
  assert.equal(config.cases.length, 20);
  assert.equal(new Set(config.cases.map((entry) => entry.id)).size, 20);
  assert.deepEqual([...new Set(config.cases.map((entry) => entry.category))].sort(), ["shape", "text"]);
  assert.deepEqual([...new Set(config.cases.map((entry) => entry.activity))].sort(), ["cycling", "running", "walking"]);
  assert.ok(new Set(config.cases.map((entry) => entry.requested_distance_metres)).size >= 4);
  for (const entry of config.cases) {
    assert.match(entry.id, /^load-(shape|text)-\d{2}$/);
    assert.ok(["normal", "cancel", "provider_timeout", "provider_quota"].includes(entry.injection));
  }
});

test("measured report exposes percentiles, distributions, and explicit release decisions", async () => {
  const report = await json("../docs/evidence/ts-403-pilot-performance.json");
  assert.equal(report.schema_version, "gpsart-pilot-report-v1");
  assert.equal(report.sample_count, 20);
  assert.equal(report.configuration, "ops/performance/ts403-workload.json");
  assert.ok(report.hardware.cpu && report.hardware.memory_gib > 0 && report.hardware.os);
  assert.match(report.provider.kind, /pinned_local_osm/);
  for (const metric of ["acknowledgement", "queue", "provider", "total"]) {
    const values = report.latency_milliseconds[metric];
    assert.ok(values.p50 <= values.p95 && values.p95 <= values.p99);
  }
  assert.equal(Object.values(report.outcomes).reduce((sum, value) => sum + value, 0), 20);
  assert.equal(Object.values(report.provider_calls).reduce((sum, value) => sum + value, 0), 20);
  for (const decision of report.release_decisions) {
    assert.ok(typeof decision.passed === "boolean");
    assert.ok(decision.observed && decision.target && decision.rationale);
  }
});

test("failure injection covers fenced recovery and base-map independence", async () => {
  const evidence = await json("../docs/evidence/ts-403-failure-injection.json");
  for (const scenario of [
    "worker_termination_during_fitting", "database_reconnect", "provider_timeout",
    "provider_quota", "lease_expiry", "base_map_slow_or_absent",
  ]) {
    assert.equal(evidence.scenarios[scenario].passed, true, `${scenario} is not proven`);
    assert.ok(evidence.scenarios[scenario].checks.length >= 2);
  }
});
