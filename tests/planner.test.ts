import assert from "node:assert/strict";
import test from "node:test";
import { distanceBounds, isCurrentResult, reviseRequest, type RouteRequest } from "../src/domain/planner.ts";
import { decodeRouteSnapshot } from "../src/domain/route-snapshot.ts";
import { routeSnapshotPayload } from "./fixtures/route-snapshot.ts";

const request: RouteRequest = { start: "Example start", activity: "Walking", design: "Heart", distanceKm: 10, tolerancePercent: 10, returnsToStart: true, revision: 3 };

test("a meaningful planner change advances the request revision", () => {
  const revised = reviseRequest(request, { distanceKm: 12 });
  assert.equal(revised.revision, 4);
  assert.equal(revised.distanceKm, 12);
  assert.equal(request.distanceKm, 10);
});

test("distance bounds preserve the requested tolerance", () => {
  assert.deepEqual(distanceBounds(10, 10), { minimumKm: 9, maximumKm: 11 });
  assert.throws(() => distanceBounds(0.5, 10), RangeError);
});

test("results from an older request revision are rejected", () => {
  assert.equal(isCurrentResult(request, 3), true);
  assert.equal(isCurrentResult(request, 2), false);
});

test("route snapshots are decoded only when every field is valid", () => {
  const valid = routeSnapshotPayload();
  assert.ok(decodeRouteSnapshot(valid));
  assert.equal(decodeRouteSnapshot({ ...valid, geometry: { ...valid.geometry, coordinates: [[77.6, 91], ...valid.geometry.coordinates.slice(1)] } }), null);
  assert.equal(decodeRouteSnapshot({ ...valid, distances: { ...valid.distances, total_metres: -1 } }), null);
});
