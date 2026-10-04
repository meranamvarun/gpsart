import assert from "node:assert/strict";
import test from "node:test";

import {
  buildCandidateComparison,
  renderCandidateDetails,
  renderCandidateResults,
  routeMapModel,
} from "../src/app/route-presentation.ts";
import { decodeRouteSnapshot, type RouteSnapshot } from "../src/domain/route-snapshot.ts";
import { routeSnapshotPayload } from "./fixtures/route-snapshot.ts";

function candidate(
  suffix: number,
  options: { artwork?: number; connector?: number; quality?: "strong_match" | "approximate_match"; offset?: number } = {},
): RouteSnapshot {
  const payload = structuredClone(routeSnapshotPayload());
  const artwork = options.artwork ?? 600;
  const connector = options.connector ?? 400;
  const offset = options.offset ?? suffix / 10_000;
  payload.id = `40000000-0000-4000-8000-${String(suffix).padStart(12, "0")}`;
  payload.candidate_version = suffix;
  payload.versions.candidate = suffix;
  payload.geometry.coordinates = [[77.6, 12.9], [77.601 + offset, 12.901], [77.6, 12.9]];
  payload.direction_markers[1]!.coordinate = [77.601 + offset, 12.901];
  payload.traversals[0]!.distance_metres = artwork;
  payload.traversals[1]!.distance_metres = connector;
  payload.distances = {
    artwork_metres: artwork,
    connector_metres: connector,
    total_metres: artwork + connector,
    repeated_metres: connector,
    repeated_included_in_total: true,
    equality_tolerance_metres: 1,
  };
  payload.quality_band = options.quality ?? "approximate_match";
  payload.duration_seconds = Math.ceil((artwork + connector) * 720 / 1000);
  const decoded = decodeRouteSnapshot(payload);
  assert.ok(decoded, "candidate fixture must satisfy the immutable route contract");
  return decoded;
}

test("one, two, and three real candidates render once without placeholder duplication", () => {
  const candidates = [candidate(1), candidate(2), candidate(3)];
  for (const count of [1, 2, 3]) {
    const comparison = buildCandidateComparison(candidates.slice(0, count), 1_050);
    assert.equal(comparison.candidates.length, count);
    const markup = renderCandidateResults(comparison, comparison.candidates[0]!.snapshot.id);
    assert.equal((markup.match(/data-candidate-id=/g) ?? []).length, count);
    for (const route of candidates.slice(0, count)) assert.equal(markup.match(new RegExp(route.id, "g"))?.length, 1);
    assert.doesNotMatch(markup, /placeholder|sample route|demo route/i);
  }
});

test("comparison labels are computed from displayed metrics", () => {
  const closest = candidate(1, { artwork: 650, connector: 400, quality: "approximate_match" });
  const bestShape = candidate(2, { artwork: 850, connector: 250, quality: "strong_match" });
  const other = candidate(3, { artwork: 700, connector: 500, quality: "approximate_match" });
  const comparison = buildCandidateComparison([closest, bestShape, other], 1_000);
  assert.equal(comparison.closestDistanceId, closest.id);
  assert.equal(comparison.bestShapeId, bestShape.id);
  assert.match(renderCandidateResults(comparison, closest.id), /Best shape match/);
  assert.match(renderCandidateResults(comparison, closest.id), /Closest distance/);
});

test("selection changes exact full geometry and direction in a north-up route model", () => {
  const first = candidate(1, { offset: 0.0001 });
  const second = candidate(2, { offset: 0.0008 });
  const comparison = buildCandidateComparison([first, second], 1_000);
  const firstMap = routeMapModel(comparison, first.id);
  const secondMap = routeMapModel(comparison, second.id);
  assert.deepEqual(firstMap.highlighted.trace, first.trace);
  assert.deepEqual(secondMap.highlighted.trace, second.trace);
  assert.deepEqual(secondMap.highlighted.directionMarkers, second.directionMarkers);
  assert.notDeepEqual(firstMap.highlighted.trace, secondMap.highlighted.trace);
  assert.equal(secondMap.northUp, true);
  assert.equal(secondMap.idealDesignOverlay, null);
});

test("details keep complete route facts and text alternatives when the base map fails", () => {
  const route = candidate(1, { artwork: 600, connector: 400 });
  const comparison = buildCandidateComparison([route], 1_000);
  const normal = renderCandidateDetails(comparison, route.id, false);
  const failed = renderCandidateDetails(comparison, route.id, true);
  for (const markup of [normal, failed]) {
    assert.match(markup, /1\.0 km/);
    assert.match(markup, /Artwork distance.*600 m/s);
    assert.match(markup, /Connectors.*400 m/s);
    assert.match(markup, /Repeated distance.*400 m.*included in total/s);
    assert.match(markup, /Return to start/);
    assert.match(markup, /forward/i);
    assert.match(markup, /reverse/i);
    assert.match(markup, /controlled-local-osm/);
    assert.match(markup, /Routing and map data can be incomplete or change/);
    assert.match(markup, /Ideal design overlay is not shown/);
  }
  assert.match(failed, /Base map unavailable/);
  assert.doesNotMatch(failed, /Route unavailable/);
});

test("identical geometry is not presented as a second candidate", () => {
  const route = candidate(1);
  const duplicate = { ...route, id: "40000000-0000-4000-8000-000000000099", version: 99 };
  const comparison = buildCandidateComparison([route, duplicate], 1_000);
  assert.equal(comparison.candidates.length, 1);
});
