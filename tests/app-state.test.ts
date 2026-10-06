import assert from "node:assert/strict";
import test from "node:test";
import { acceptApiResult, acceptCatalogueLoad, initialState, updateRequest } from "../src/app/state.ts";
import { routeSnapshot } from "./fixtures/route-snapshot.ts";

const snapshot = routeSnapshot();

test("route-affecting input has one revisioned model", () => {
  const state = initialState();
  for (const change of [
    { start: "12.97160, 77.59460" },
    { activity: "Walking" as const },
    { design: "Circle" },
    { distanceKm: 12 },
    { tolerancePercent: 20 },
    { returnsToStart: false },
  ]) {
    const changed = updateRequest(state, change);
    assert.equal(changed.request.revision, state.request.revision + 1);
    for (const [key, value] of Object.entries(change)) {
      assert.equal(changed.request[key as keyof typeof changed.request], value);
    }
  }
});

test("decoded results enter state only when their request revision is current", () => {
  const state = updateRequest(initialState(), { distanceKm: 10 });
  const accepted = acceptApiResult(state, { kind: "route", snapshot });
  assert.equal(accepted.routeResult?.id, snapshot.id);
  const revised = updateRequest(accepted, { distanceKm: 11 });
  assert.equal(acceptApiResult(revised, { kind: "route", snapshot }).routeResult, null);
});

test("non-route outcomes are explicit and never synthesize a candidate", () => {
  const state = initialState();
  assert.equal(acceptApiResult(state, { kind: "no_match" }).outcome, "no_match");
  assert.equal(acceptApiResult(state, { kind: "unavailable" }).outcome, "unavailable");
  assert.equal(acceptApiResult(state, { kind: "invalid_response" }).outcome, "invalid_response");
});

test("design changes preserve the remaining planner form", () => {
  const loaded = acceptCatalogueLoad(initialState(), { kind: "available", designs: [] });
  const before = updateRequest(loaded, { start: "12.97, 77.59", activity: "Walking", distanceKm: 8 });
  const after = updateRequest(before, { design: "HI" });
  assert.equal(after.request.start, before.request.start);
  assert.equal(after.request.activity, before.request.activity);
  assert.equal(after.request.distanceKm, before.request.distanceKm);
  assert.equal(after.request.design, "HI");
});
