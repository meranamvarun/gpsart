import assert from "node:assert/strict";
import test from "node:test";
import { decodeApiRouteResult } from "../src/client/api.ts";
import { routeSnapshot, routeSnapshotPayload } from "./fixtures/route-snapshot.ts";

const snapshot = routeSnapshot();
const payload = routeSnapshotPayload();

test("API responses are decoded before route state can be used", () => {
  assert.deepEqual(decodeApiRouteResult({ kind: "route", snapshot: payload }), { kind: "route", snapshot });
  assert.deepEqual(decodeApiRouteResult({ kind: "route", snapshot: { ...payload, distances: { ...payload.distances, total_metres: -1 } } }), { kind: "invalid_response" });
  assert.deepEqual(decodeApiRouteResult({ kind: "no_match" }), { kind: "no_match" });
});
