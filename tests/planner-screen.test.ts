import assert from "node:assert/strict";
import test from "node:test";

import { renderScreen } from "../src/app/screen-controller.ts";
import { acceptCatalogueLoad, initialState } from "../src/app/state.ts";
import type { PlannerWorkflowSnapshot } from "../src/client/planner-workflow.ts";

test("generation progress is labelled with the active job request, not a newer edit", () => {
  const state = initialState();
  const newer = {
    ...state,
    request: { ...state.request, start: "newer synthetic start", revision: 2 },
  };
  const activeRequest = { ...newer.request, start: "older synthetic start", revision: 1 };
  const workflow: PlannerWorkflowSnapshot = {
    phase: "polling",
    request: activeRequest,
    proposedStart: {
      point: { latitude: 0.5, longitude: -150.25 },
      source: "search",
      label: activeRequest.start,
    },
    resolution: null,
    jobId: "3f8b4f31-ee11-4f11-87aa-b4f15fc00cf1",
    status: null,
    previousStatus: null,
    issue: "active_job",
  };
  const markup = renderScreen("generating", newer, { kind: "available", routes: [], errors: [] }, workflow);
  assert.match(markup, /older synthetic start/);
  assert.doesNotMatch(markup, /newer synthetic start/);
  assert.match(markup, /earlier request is still active/i);
});

test("catalogue exposes selected state and explains disabled designs without changing the form", () => {
  const original = { ...initialState(), request: { ...initialState().request, start: "12.97, 77.59", activity: "Walking" as const, distanceKm: 8 } };
  const state = acceptCatalogueLoad(original, { kind: "available", designs: [{
    id: "heart", version: 1, status: "published", category: "shape", name: "Heart", accessibleName: "Heart route design",
    complexityHint: "simple", previewSvg: "<svg></svg>", definitionHash: "a".repeat(64),
  }] });
  const markup = renderScreen("catalogue", state, { kind: "available", routes: [], errors: [] });
  assert.match(markup, /aria-label="Heart route design, Easy"[^>]*aria-pressed="true"/);
  assert.match(markup, /data-design="Circle"[^>]*disabled/);
  assert.match(markup, /Circle is not available because it has no reviewed legal routing fixture/i);
  assert.equal(state.request.start, original.request.start);
  assert.equal(state.request.activity, original.request.activity);
  assert.equal(state.request.distanceKm, original.request.distanceKm);
});
