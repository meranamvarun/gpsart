import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const readJson = async (path) => JSON.parse(await readFile(new URL(path, import.meta.url), "utf8"));

test("release checklist is traceable and fail-closed", async () => {
  const checklist = await readJson("../docs/release/first-release-checklist.json");
  assert.equal(checklist.schema_version, "gpsart-release-checklist-v1");
  assert.equal(checklist.decision, "no_go");
  const required = ["held_out_benchmark", "route_validity", "browser_journey", "provider_obligations", "performance", "accessibility", "security_privacy", "gpx_import"];
  assert.deepEqual(checklist.gates.map((gate) => gate.id).sort(), required.sort());
  assert.ok(checklist.gates.some((gate) => gate.status !== "passed"));
  assert.equal(
    checklist.gates.find((gate) => gate.id === "provider_obligations")?.status,
    "blocked",
    "provider obligations cannot pass without a reviewed deployed configuration and measured cost",
  );
  assert.deepEqual(
    checklist.signatures.map((signature) => signature.role).sort(),
    ["developer", "qa"],
    "the NO-GO decision requires developer and independent QA signatures",
  );
  for (const gate of checklist.gates) {
    assert.ok(gate.evidence.length > 0);
    for (const path of gate.evidence) await access(new URL(`../${path}`, import.meta.url));
  }
});

test("missing held-out reviews and browsers remain explicit blockers", async () => {
  const benchmark = await readJson("../docs/evidence/ts-404-held-out-benchmark.json");
  assert.equal(benchmark.requirements.feasible_total, 50);
  assert.equal(benchmark.requirements.shape_minimum, 20);
  assert.equal(benchmark.requirements.text_minimum, 20);
  assert.equal(benchmark.requirements.difficult_minimum, 20);
  assert.equal(benchmark.observed.independently_reviewed_feasible, 0);
  assert.equal(benchmark.release_gate_passed, false);
  const browsers = await readJson("../docs/evidence/ts-404-browser-matrix.json");
  assert.deepEqual(browsers.targets.map((entry) => entry.browser), ["Chrome", "Firefox", "Safari", "Mobile Chrome", "Mobile Safari"]);
  assert.ok(browsers.targets.some((entry) => entry.status !== "passed"));
  assert.equal(browsers.release_gate_passed, false);
});

test("source-configured OpenStreetMap tiles use the reviewed endpoint and linked attribution", async () => {
  const app = await readFile(new URL("../src/app.ts", import.meta.url), "utf8");
  assert.doesNotMatch(app, /\{s\}\.tile\.openstreetmap\.org/);
  assert.match(app, /https:\/\/tile\.openstreetmap\.org\/\{z\}\/\{x\}\/\{y\}\.png/);
  assert.match(app, /https:\/\/www\.openstreetmap\.org\/copyright/);
  assert.match(app, /OpenStreetMap contributors/);
});
