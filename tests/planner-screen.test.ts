import assert from "node:assert/strict";
import test from "node:test";

import { renderScreen } from "../src/app/screen-controller.ts";
import { initialState } from "../src/app/state.ts";
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
  const markup = renderScreen("generating", newer, { kind: "available", snapshots: [] }, workflow);
  assert.match(markup, /older synthetic start/);
  assert.doesNotMatch(markup, /newer synthetic start/);
  assert.match(markup, /earlier request is still active/i);
});
