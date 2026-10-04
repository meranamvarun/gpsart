import assert from "node:assert/strict";
import test from "node:test";
import { decodeApiRouteResult } from "../src/client/api.ts";
import { loadRouteSnapshots, ROUTE_SNAPSHOT_STORAGE_KEY, saveRouteSnapshots } from "../src/client/storage.ts";
import { routeSnapshot, routeSnapshotPayload } from "./fixtures/route-snapshot.ts";

const snapshot = routeSnapshot();
const payload = routeSnapshotPayload();

test("API responses are decoded before route state can be used", () => {
  assert.deepEqual(decodeApiRouteResult({ kind: "route", snapshot: payload }), { kind: "route", snapshot });
  assert.deepEqual(decodeApiRouteResult({ kind: "route", snapshot: { ...payload, distances: { ...payload.distances, total_metres: -1 } } }), { kind: "invalid_response" });
  assert.deepEqual(decodeApiRouteResult({ kind: "no_match" }), { kind: "no_match" });
});

test("malformed or legacy browser records have an explicit unsupported state", () => {
  const storage = { getItem: () => "{not JSON", setItem: () => undefined };
  assert.deepEqual(loadRouteSnapshots(storage), { kind: "unsupported_snapshot" });
});

test("valid current browser records remain available", () => {
  const storage = { getItem: (key: string) => key === ROUTE_SNAPSHOT_STORAGE_KEY ? JSON.stringify([payload]) : null, setItem: () => undefined };
  assert.deepEqual(loadRouteSnapshots(storage), { kind: "available", snapshots: [snapshot] });
});

test("current snapshots retain the complete immutable contract across storage", () => {
  let stored: string | null = null;
  const storage = { getItem: () => stored, setItem: (_key: string, value: string) => { stored = value; } };
  assert.equal(saveRouteSnapshots(storage, [snapshot]), true);
  assert.deepEqual(loadRouteSnapshots(storage), { kind: "available", snapshots: [snapshot] });
});
