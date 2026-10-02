import assert from "node:assert/strict";
import test from "node:test";
import { decodeApiRouteResult } from "../src/client/api.ts";
import { loadRouteSnapshots, ROUTE_SNAPSHOT_STORAGE_KEY } from "../src/client/storage.ts";

const snapshot = { id: "route-1", version: 1, requestRevision: 0, totalMetres: 1000, trace: [{ latitude: 12.9, longitude: 77.6 }] };

test("API responses are decoded before route state can be used", () => {
  assert.deepEqual(decodeApiRouteResult({ kind: "route", snapshot }), { kind: "route", snapshot });
  assert.deepEqual(decodeApiRouteResult({ kind: "route", snapshot: { ...snapshot, totalMetres: -1 } }), { kind: "invalid_response" });
  assert.deepEqual(decodeApiRouteResult({ kind: "no_match" }), { kind: "no_match" });
});

test("malformed or legacy browser records have an explicit unsupported state", () => {
  const storage = { getItem: () => "{not JSON", setItem: () => undefined };
  assert.deepEqual(loadRouteSnapshots(storage), { kind: "unsupported_snapshot" });
});

test("valid current browser records remain available", () => {
  const storage = { getItem: (key: string) => key === ROUTE_SNAPSHOT_STORAGE_KEY ? JSON.stringify([snapshot]) : null, setItem: () => undefined };
  assert.deepEqual(loadRouteSnapshots(storage), { kind: "available", snapshots: [snapshot] });
});
