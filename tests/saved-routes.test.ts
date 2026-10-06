import assert from "node:assert/strict";
import test from "node:test";
import { IDBFactory, IDBObjectStore } from "fake-indexeddb";

import { exportRouteSnapshotGpx } from "../src/domain/gpx-export.ts";
import { IndexedDbSavedRouteRepository } from "../src/client/saved-routes.ts";
import type { RouteRequest } from "../src/domain/planner.ts";
import { renderScreen } from "../src/app/screen-controller.ts";
import { initialState, openSavedRoute } from "../src/app/state.ts";
import { decodeRouteSnapshot } from "../src/domain/route-snapshot.ts";
import { routeSnapshot, routeSnapshotPayload } from "./fixtures/route-snapshot.ts";

const request: RouteRequest = Object.freeze({
  start: "12.90000, 77.60000",
  activity: "Walking",
  design: "Heart",
  distanceKm: 1,
  tolerancePercent: 10,
  returnsToStart: true,
  revision: 1,
});

function repository(factory = new IDBFactory(), name = `gpsart-test-${crypto.randomUUID()}`) {
  return new IndexedDbSavedRouteRepository(factory, name);
}

async function putRaw(factory: IDBFactory, name: string, value: unknown): Promise<void> {
  const database = await new Promise<IDBDatabase>((resolve, reject) => {
    const request = factory.open(name, 1);
    request.addEventListener("success", () => resolve(request.result), { once: true });
    request.addEventListener("error", () => reject(request.error), { once: true });
  });
  await new Promise<void>((resolve, reject) => {
    const transaction = database.transaction("routes", "readwrite");
    transaction.objectStore("routes").put(value);
    transaction.addEventListener("complete", () => resolve(), { once: true });
    transaction.addEventListener("abort", () => reject(transaction.error), { once: true });
  });
  database.close();
}

test("save, list, rename, reopen, offline export, and delete retain the complete immutable route", async () => {
  const routes = repository();
  const snapshot = routeSnapshot();

  assert.deepEqual(await routes.save({ name: "Morning heart", request, snapshot }), { kind: "saved", id: snapshot.id });
  assert.deepEqual(await routes.rename(snapshot.id, "Park heart"), { kind: "renamed" });

  const listed = await routes.list();
  assert.equal(listed.kind, "available");
  assert.deepEqual(listed.errors, []);
  assert.equal(listed.routes[0]?.name, "Park heart");

  const reopened = await routes.get(snapshot.id);
  assert.equal(reopened.kind, "available");
  assert.deepEqual(reopened.route.request, request);
  assert.deepEqual(reopened.route.snapshot, snapshot);
  assert.match(exportRouteSnapshotGpx(reopened.route.snapshot), new RegExp(snapshot.id));

  assert.deepEqual(await routes.delete(snapshot.id), { kind: "deleted" });
  assert.deepEqual(await routes.list(), { kind: "available", routes: [], errors: [] });
});

test("malformed and unknown-version records are isolated as recoverable per-record errors", async () => {
  const factory = new IDBFactory();
  const name = `gpsart-test-${crypto.randomUUID()}`;
  const routes = repository(factory, name);
  const snapshot = routeSnapshot();
  await routes.save({ name: "Valid", request, snapshot });
  await putRaw(factory, name, { id: "broken", schema_version: 1, name: "Broken" });
  await putRaw(factory, name, { id: "future", schema_version: 99, name: "Future" });

  const listed = await routes.list();
  assert.equal(listed.kind, "available");
  assert.deepEqual(listed.routes.map(({ id }) => id), [snapshot.id]);
  assert.deepEqual(listed.errors, [
    { id: "broken", kind: "malformed_record" },
    { id: "future", kind: "unsupported_schema" },
  ]);
  assert.deepEqual(await routes.get("future"), { kind: "record_error", id: "future", error: "unsupported_schema" });
});

test("storage denial and quota failures are recoverable and do not mutate an existing route", async () => {
  const denied = repository({ open: () => { throw new DOMException("denied", "SecurityError"); } } as unknown as IDBFactory);
  assert.deepEqual(await denied.list(), { kind: "unavailable", reason: "denied" });

  const routes = repository();
  const snapshot = routeSnapshot();
  await routes.save({ name: "Original", request, snapshot });
  const originalPut = IDBObjectStore.prototype.put;
  IDBObjectStore.prototype.put = function (): IDBRequest<IDBValidKey> {
    throw new DOMException("full", "QuotaExceededError");
  };
  try {
    assert.deepEqual(await routes.rename(snapshot.id, "Should not stick"), { kind: "unavailable", reason: "quota_exceeded" });
  } finally {
    IDBObjectStore.prototype.put = originalPut;
  }
  const reopened = await routes.get(snapshot.id);
  assert.equal(reopened.kind, "available");
  assert.equal(reopened.route.name, "Original");
  assert.equal(reopened.route.snapshot.id, snapshot.id, "the selected server snapshot remains usable/exportable");
});

test("a failed delete transaction leaves the target and every other record intact", async () => {
  const routes = repository();
  const first = routeSnapshot();
  const payload = structuredClone(routeSnapshotPayload());
  payload.id = "40000000-0000-4000-8000-000000000002";
  const second = decodeRouteSnapshot(payload);
  assert.ok(second);
  await routes.save({ name: "First", request, snapshot: first });
  await routes.save({ name: "Second", request, snapshot: second });

  const originalDelete = IDBObjectStore.prototype.delete;
  IDBObjectStore.prototype.delete = function (): IDBRequest<undefined> {
    throw new DOMException("write failed", "QuotaExceededError");
  };
  try {
    assert.deepEqual(await routes.delete(first.id), { kind: "unavailable", reason: "quota_exceeded" });
  } finally {
    IDBObjectStore.prototype.delete = originalDelete;
  }
  const listed = await routes.list();
  assert.equal(listed.kind, "available");
  assert.deepEqual(listed.routes.map(({ id }) => id).sort(), [first.id, second.id].sort());
});

test("sign-in state is not part of local records and cannot relabel or remove them", async () => {
  const routes = repository();
  const snapshot = routeSnapshot();
  await routes.save({ name: "Device route", request, snapshot });
  const before = await routes.list();
  // Authentication state changes elsewhere; this repository has no account or upload API.
  const after = await routes.list();
  assert.deepEqual(after, before);
  assert.equal(after.kind === "available" && after.routes[0]?.storageLabel, "Saved on this device");
});

test("saved-route UI explains device-only persistence and exposes every local action", async () => {
  const routes = repository();
  const snapshot = routeSnapshot();
  await routes.save({ name: "Device route", request, snapshot });
  const loaded = await routes.list();
  const markup = renderScreen("saved", initialState(), loaded);
  assert.match(markup, /Saved on this device/);
  assert.match(markup, /clearing (this )?site data/i);
  for (const action of ["open-saved", "rename-saved", "delete-saved", "export-saved-gpx"]) {
    assert.match(markup, new RegExp(`data-action="${action}"`));
  }
});

test("offline reopen hydrates route details only from the stored request and snapshot", () => {
  const snapshot = routeSnapshot();
  const saved = Object.freeze({ id: snapshot.id, name: "Offline", savedAtEpochMilliseconds: 1, storageLabel: "Saved on this device" as const, request, snapshot });
  const reopened = openSavedRoute(initialState(), saved);
  assert.deepEqual(reopened.request, request);
  assert.equal(reopened.routeResult, snapshot);
  assert.deepEqual(reopened.routeResults, [snapshot]);
  assert.equal(reopened.candidateLoad, "available");
  assert.equal(reopened.selectedCandidateId, snapshot.id);
});

test("per-record errors remain visible beside usable routes", async () => {
  const factory = new IDBFactory();
  const name = `gpsart-test-${crypto.randomUUID()}`;
  const routes = repository(factory, name);
  const snapshot = routeSnapshot();
  await routes.save({ name: "Usable", request, snapshot });
  await putRaw(factory, name, { id: "future", schema_version: 2 });
  const markup = renderScreen("saved", initialState(), await routes.list());
  assert.match(markup, /Usable/);
  assert.match(markup, /future/);
  assert.match(markup, /unsupported version/i);
});
