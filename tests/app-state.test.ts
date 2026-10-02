import assert from "node:assert/strict";
import test from "node:test";
import { acceptApiResult, initialState, updateRequest } from "../src/app/state.ts";

const snapshot = { id: "route-1", version: 1, requestRevision: 0, totalMetres: 1_000, trace: [{ latitude: 10, longitude: 20 }] } as const;

test("route-affecting input has one revisioned model", () => {
  const state = initialState();
  const changed = updateRequest(state, { design: "Circle", distanceKm: 12 });
  assert.equal(changed.request.design, "Circle");
  assert.equal(changed.request.distanceKm, 12);
  assert.equal(changed.request.revision, state.request.revision + 1);
});

test("decoded results enter state only when their request revision is current", () => {
  const state = initialState();
  const accepted = acceptApiResult(state, { kind: "route", snapshot });
  assert.equal(accepted.routeResult?.id, "route-1");
  const revised = updateRequest(accepted, { distanceKm: 11 });
  assert.equal(acceptApiResult(revised, { kind: "route", snapshot }).routeResult, null);
});

test("non-route outcomes are explicit and never synthesize a candidate", () => {
  const state = initialState();
  assert.equal(acceptApiResult(state, { kind: "no_match" }).outcome, "no_match");
  assert.equal(acceptApiResult(state, { kind: "unavailable" }).outcome, "unavailable");
  assert.equal(acceptApiResult(state, { kind: "invalid_response" }).outcome, "invalid_response");
});
