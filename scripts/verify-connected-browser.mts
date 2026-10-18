import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import { routeMapModel, buildCandidateComparison, renderCandidateDetails } from "../src/app/route-presentation.ts";
import { acceptCandidateLoad, beginCandidateLoad, initialState } from "../src/app/state.ts";
import { decodeRouteSnapshot } from "../src/domain/route-snapshot.ts";

const transcriptPath = process.argv[2];
assert.ok(transcriptPath, "the connected Rust test must provide its ephemeral API transcript");
const payload = JSON.parse(await readFile(transcriptPath, "utf8"));
const snapshot = decodeRouteSnapshot(payload);
assert.ok(snapshot, "the browser decoder must accept the live production-path API payload");

const loading = beginCandidateLoad(initialState(), snapshot.requestRevision);
const loaded = acceptCandidateLoad(loading, snapshot.requestRevision, {
  kind: "available",
  snapshots: [snapshot],
});
assert.equal(loaded.candidateLoad, "available");
assert.equal(loaded.selectedCandidateId, snapshot.id);

const comparison = buildCandidateComparison(loaded.routeResults, 1_300);
const map = routeMapModel(comparison, snapshot.id);
const apiCoordinates = payload.geometry.coordinates.map(([longitude, latitude]: [number, number]) => ({
  longitude,
  latitude,
}));
assert.ok(
  map.highlighted.trace.length === apiCoordinates.length &&
    map.highlighted.trace.every(
      (point, index) =>
        point.longitude === apiCoordinates[index]?.longitude &&
        point.latitude === apiCoordinates[index]?.latitude,
    ),
  "rendered and API coordinate order must match",
);
assert.equal(map.highlighted.id, payload.id);
assert.equal(snapshot.generationJobId, payload.generation_job_id);
assert.equal(snapshot.requestRevision, payload.request_revision);
assert.equal(snapshot.totalMetres, payload.distances.total_metres);
assert.equal(snapshot.repeatedMetres, payload.distances.repeated_metres);
assert.deepEqual(snapshot.versions, {
  snapshot: payload.versions.snapshot,
  request: payload.versions.request,
  profile: payload.versions.profile,
  templateId: payload.versions.template_id,
  template: payload.versions.template,
  templateHash: payload.versions.template_hash,
  algorithm: payload.versions.algorithm,
  routingData: payload.versions.routing_data,
  routingPolicy: payload.versions.routing_policy,
  candidate: payload.versions.candidate,
});

const details = renderCandidateDetails(comparison, snapshot.id);
assert.match(details, /Route facts/);
assert.match(details, new RegExp(`${snapshot.totalMetres} m|${(snapshot.totalMetres / 1000).toFixed(1)} km`));
assert.match(details, new RegExp(`${snapshot.repeatedMetres} m`));
assert.match(details, /included in total/);
assert.match(details, new RegExp(snapshot.versions.algorithm));
assert.match(details, new RegExp(snapshot.versions.routingPolicy));
assert.match(details, new RegExp(snapshot.versions.routingData.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));

console.log("Live API route decoded and rendered with exact identity, geometry, distances, retracing, and version pins.");
