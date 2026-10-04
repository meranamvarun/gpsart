import assert from "node:assert/strict";
import test from "node:test";

import { fetchRouteCandidates } from "../src/client/api.ts";
import { baseMapTileErrorHandler, renderMap } from "../src/app/map.ts";
import { renderScreen } from "../src/app/screen-controller.ts";
import {
  acceptCandidateLoad,
  beginCandidateLoad,
  initialState,
  selectCandidate,
} from "../src/app/state.ts";
import { routeSnapshot, routeSnapshotPayload } from "./fixtures/route-snapshot.ts";

const json = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status, headers: { "Content-Type": "application/json" } });

test("candidate snapshots are fetched exactly by result id and decoded atomically", async () => {
  const ids = [
    "40000000-0000-4000-8000-000000000001",
    "40000000-0000-4000-8000-000000000002",
  ];
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const result = await fetchRouteCandidates(ids, async (input, init) => {
    const url = String(input);
    calls.push({ url, init });
    const payload = structuredClone(routeSnapshotPayload());
    const index = ids.indexOf(url.split("/").at(-1)!);
    payload.id = ids[index]!;
    payload.candidate_version = index + 1;
    payload.versions.candidate = index + 1;
    payload.geometry.coordinates[1]![0] += index / 10_000;
    payload.direction_markers[1]!.coordinate[0] += index / 10_000;
    return json(payload);
  });
  assert.equal(result.kind, "available");
  assert.deepEqual(result.kind === "available" ? result.snapshots.map(({ id }) => id) : [], ids);
  assert.deepEqual(calls.map(({ url }) => url), ids.map((id) => `/api/v1/routes/${id}`));
  assert.ok(calls.every(({ init }) => init?.credentials === "same-origin" && init.cache === "no-store"));
});

test("a missing or malformed route never becomes a partial comparison", async () => {
  const ids = ["40000000-0000-4000-8000-000000000001", "40000000-0000-4000-8000-000000000002"];
  const missing = await fetchRouteCandidates(ids, async (input) => String(input).endsWith("2") ? json({}, 404) : json(routeSnapshotPayload()));
  assert.deepEqual(missing, { kind: "unavailable" });
  const malformed = await fetchRouteCandidates([ids[0]!], async () => json({ ...routeSnapshotPayload(), geometry: null }));
  assert.deepEqual(malformed, { kind: "invalid_response" });
  const invalidJson = await fetchRouteCandidates([ids[0]!], async () => new Response("{bad json", { status: 200 }));
  assert.deepEqual(invalidJson, { kind: "invalid_response" });
});

test("loaded candidates drive result cards, details, and selected route map", () => {
  const snapshot = routeSnapshot();
  const loading = beginCandidateLoad(initialState(), snapshot.requestRevision);
  const loaded = acceptCandidateLoad(loading, snapshot.requestRevision, { kind: "available", snapshots: [snapshot] });
  assert.equal(loaded.candidateLoad, "available");
  assert.equal(loaded.selectedCandidateId, snapshot.id);
  assert.match(renderScreen("results", loaded, { kind: "available", snapshots: [] }), new RegExp(snapshot.id));
  assert.match(renderScreen("details", loaded, { kind: "available", snapshots: [] }), /Route facts/);
  assert.match(renderMap("results", loaded), /id="route-map"/);
  assert.match(renderMap("results", loaded), /North-up comparison/);
  assert.equal(selectCandidate(loaded, "not-owned"), loaded);
});

test("stale candidate responses cannot replace a newer load", () => {
  const snapshot = routeSnapshot();
  const loading = beginCandidateLoad(initialState(), snapshot.requestRevision + 1);
  assert.equal(acceptCandidateLoad(loading, snapshot.requestRevision, { kind: "available", snapshots: [snapshot] }), loading);
});

test("an older successful completion cannot even begin replacing newer candidates", () => {
  const snapshot = routeSnapshot();
  const newer = acceptCandidateLoad(beginCandidateLoad(initialState(), 2), 2, {
    kind: "available",
    snapshots: [{ ...snapshot, requestRevision: 2 }],
  });
  assert.equal(beginCandidateLoad(newer, 1), newer);
  assert.equal(newer.candidateRequestRevision, 2);
});

test("the production tile-error handler reports only the base map as unavailable", () => {
  const status = { innerHTML: "Complete routed geometry · north up" };
  const handleTileError = baseMapTileErrorHandler(status);
  handleTileError();
  assert.match(status.innerHTML, /Base map unavailable/);
  assert.match(status.innerHTML, /Complete routed geometry remains visible/);
  assert.doesNotMatch(status.innerHTML, /Route unavailable/);
  const firstMessage = status.innerHTML;
  handleTileError();
  assert.equal(status.innerHTML, firstMessage);
});
