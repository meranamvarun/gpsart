import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("the production worker delegates take_work to the durable routing pipeline", async () => {
  const [library, main, production] = await Promise.all([
    source("../crates/gpsart-worker/src/lib.rs"),
    source("../crates/gpsart-worker/src/main.rs"),
    source("../crates/gpsart-worker/src/production.rs"),
  ]);
  assert.match(library, /worker\.take_one\(\)\.await/);
  assert.doesNotMatch(library, /take_work:\s*Arc::new\(\|\|\s*Box::pin\(async\s*\{\s*Ok\(\(\)\)\s*\}\)\)/);
  assert.match(main, /ProductionWorker::connect/);
  assert.match(production, /load_claimed_input/);
  assert.match(production, /publish_success/);
  assert.match(production, /CandidateSearch::new/);
  assert.match(production, /connect_with_limits/);
  assert.match(production, /JobStateUnavailable/);
});

test("production routing requires external checksum-pinned graph inputs", async () => {
  const [main, example] = await Promise.all([
    source("../crates/gpsart-worker/src/main.rs"),
    source("../ops/worker.env.example"),
  ]);
  for (const variable of [
    "GPSART_COVERAGE_VERSION",
    "GPSART_ROUTING_GRAPH_PATH",
    "GPSART_ROUTING_GRAPH_SHA256_PATH",
  ]) {
    assert.match(main, new RegExp(variable));
    assert.match(example, new RegExp(variable));
  }
});

test("the connected production slice is a mandatory PostGIS CI gate", async () => {
  const [packageJson, workflow, connectedTest, runner, browserVerifier] = await Promise.all([
    source("../package.json"),
    source("../.github/workflows/ci.yml"),
    source("../crates/gpsart-worker/tests/connected_vertical_slice.rs"),
    source("../scripts/test-connected.mjs"),
    source("../scripts/verify-connected-browser.mts"),
  ]);
  const scripts = JSON.parse(packageJson).scripts;
  assert.equal(scripts["test:connected"], "node scripts/test-connected.mjs");
  assert.match(workflow, /- run: npm run test:connected/);
  assert.match(connectedTest, /DATABASE_URL.*expect\(/s);
  assert.doesNotMatch(connectedTest, /skipping|return None/);
  assert.match(connectedTest, /ProductionRoutingConfig::load/);
  assert.match(connectedTest, /ProductionWorker::connect/);
  assert.match(connectedTest, /gpx::read/);
  assert.match(connectedTest, /GPSART_CONNECTED_TRANSCRIPT_PATH/);
  assert.match(runner, /mkdtemp/);
  assert.match(runner, /connected_vertical_slice/);
  assert.match(runner, /verify-connected-browser\.mts/);
  assert.match(runner, /await rm\(temporaryDirectory/);
  assert.match(browserVerifier, /decodeRouteSnapshot/);
  assert.match(browserVerifier, /routeMapModel/);
  assert.match(browserVerifier, /renderCandidateDetails/);
});
