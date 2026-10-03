import assert from "node:assert/strict";
import test from "node:test";
import {
  decodeConfirmedStart,
  decodeCoverageResponse,
  decodeStartOutcome,
  decodeStartResolution,
} from "../src/domain/start-resolution";

const valid = {
  id: "7bf80ccf-68e8-49ec-a3e8-cad667bb95a0",
  requested_point: { longitude: 77.2, latitude: 28.6 },
  candidate_network_point: { longitude: 77.2001, latitude: 28.6001 },
  snap_distance_metres: 30,
  profile_version: "walking-v1",
  coverage_policy_version: "coverage-v1",
  data_version: "map-v1",
  expires_at_epoch_seconds: 1_800_000_000,
  confirmation_required: true,
};

test("start resolution decoder preserves requested and candidate coordinates", () => {
  assert.deepEqual(decodeStartResolution(valid), valid);
});

test("coverage, confirmation, and machine outcomes are decoded explicitly", () => {
  assert.deepEqual(
    decodeCoverageResponse({
      version: "coverage-v1",
      geojson: { type: "Feature", geometry: { type: "Polygon", coordinates: [] } },
      local_route_feasibility: "not_evaluated",
    })?.version,
    "coverage-v1",
  );
  assert.deepEqual(
    decodeConfirmedStart({ confirmed_start: { longitude: 77.2, latitude: 28.6 } }),
    { longitude: 77.2, latitude: 28.6 },
  );
  assert.equal(decodeStartOutcome({ outcome: { code: "no_routable_access" } }), "no_routable_access");
  assert.equal(decodeStartOutcome({ outcome: { code: "storage_unavailable" } }), "storage_unavailable");
  assert.equal(decodeStartOutcome({ outcome: { code: "nearest-ish" } }), undefined);
  assert.equal(decodeCoverageResponse({ version: "v1", geojson: null }), undefined);
});

test("start resolution decoder rejects unknown, invalid, or implicitly confirmed data", () => {
  assert.equal(decodeStartResolution({ ...valid, profile_version: "walking-latest" }), undefined);
  assert.equal(decodeStartResolution({ ...valid, snap_distance_metres: -1 }), undefined);
  assert.equal(decodeStartResolution({ ...valid, confirmation_required: false }), undefined);
  assert.equal(
    decodeStartResolution({ ...valid, requested_point: { longitude: 200, latitude: 28.6 } }),
    undefined,
  );
});
