import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);
const read = (path) => readFile(new URL(path, root), "utf8");

test("forward-only SQLx migrations encode the TS-002 persistence contract", async () => {
  const files = (await readdir(new URL("migrations/", root))).sort();
  assert.deepEqual(files, [
    "0001_foundation.sql",
    "0002_route_jobs.sql",
    "0003_guest_session_digest.sql",
  ]);
  const sql = await Promise.all(files.map((file) => read(`migrations/${file}`)));
  const schema = sql.join("\n");

  assert.match(schema, /CREATE EXTENSION IF NOT EXISTS postgis/i);
  for (const entity of [
    "guest_sessions",
    "resolved_starts",
    "planning_requests",
    "generation_jobs",
    "job_attempts",
    "route_candidates",
    "cleanup_runs",
  ]) assert.match(schema, new RegExp(`CREATE TABLE ${entity}`, "i"));

  for (const state of ["queued", "running", "succeeded", "no_match", "failed", "cancelled", "timed_out"]) {
    assert.match(schema, new RegExp(`'${state}'`));
  }
  assert.match(schema, /UNIQUE\s*\(guest_session_id, idempotency_key\)/i);
  assert.match(schema, /geometry\s*\(Point,\s*4326\)/i);
  assert.match(schema, /geometry\s*\(LineString,\s*4326\)/i);
  assert.match(schema, /requested_distance_metres/i);
  assert.match(schema, /activity_profile_version/i);
  assert.match(schema, /template_version/i);
  assert.match(schema, /map_data_version/i);
  assert.match(schema, /algorithm_version/i);
  assert.doesNotMatch(schema, /DROP\s+(TABLE|COLUMN|TYPE)/i);
});

test("both binaries validate the SQLx migration set before starting", async () => {
  const persistence = await read("crates/gpsart-persistence/src/lib.rs");
  const api = await read("crates/gpsart-api/src/main.rs");
  const worker = await read("crates/gpsart-worker/src/lib.rs");
  assert.match(persistence, /MigrationStatus/);
  assert.match(persistence, /_sqlx_migrations/);
  assert.match(api, /check\(\)\.await/);
  assert.match(worker, /probe[\s\S]*\.check\(\)[\s\S]*\.await/);
});

test("entity ownership, retention, and GeoJSON conversion are documented", async () => {
  const document = await read("docs/persistence-entities-and-retention.md");
  for (const phrase of ["24 hours", "SRID 4326", "ST_AsGeoJSON", "ST_GeomFromGeoJSON", "guest_session_id", "immutable"]) {
    assert.match(document, new RegExp(phrase, "i"));
  }
});
