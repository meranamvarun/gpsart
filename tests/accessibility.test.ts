import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { renderScreen } from "../src/app/screen-controller.ts";
import { initialState } from "../src/app/state.ts";
import type { PlannerWorkflowSnapshot } from "../src/client/planner-workflow.ts";
import { routeSnapshot } from "./fixtures/route-snapshot.ts";

const savedRoutes = { kind: "available" as const, routes: [], errors: [] };

function invalidPlannerSnapshot(): PlannerWorkflowSnapshot {
  return {
    phase: "error",
    request: initialState().request,
    proposedStart: null,
    resolution: null,
    jobId: null,
    status: null,
    previousStatus: null,
    issue: "invalid_fields",
  };
}

test("planner errors are associated with their fields and expose non-colour state", () => {
  const markup = renderScreen("planner", initialState(), savedRoutes, invalidPlannerSnapshot());

  assert.match(markup, /id="start"[^>]*aria-describedby="location-status start-error"[^>]*aria-invalid="true"/);
  assert.match(markup, /id="start-error"[^>]*role="alert"[^>]*>[^<]*starting point/i);
  assert.match(markup, /id="distance"[^>]*aria-describedby="distance-help distance-error"[^>]*aria-invalid="true"/);
  assert.match(markup, /id="distance-error"[^>]*role="alert"[^>]*>[^<]*distance/i);
});

test("design tabs expose a keyboard-operable tab and tabpanel relationship", () => {
  const markup = renderScreen("planner", initialState(), savedRoutes);

  assert.match(markup, /role="tab"[^>]*id="design-tab-shapes"[^>]*aria-controls="design-panel"[^>]*tabindex="0"/);
  assert.match(markup, /role="tab"[^>]*id="design-tab-text"[^>]*aria-controls="design-panel"[^>]*tabindex="-1"/);
  assert.match(markup, /id="design-panel"[^>]*role="tabpanel"[^>]*aria-labelledby="design-tab-shapes"/);
});

test("progress has named step semantics without a repeatedly recreated live region", () => {
  const state = initialState();
  const workflow: PlannerWorkflowSnapshot = {
    phase: "polling",
    request: state.request,
    proposedStart: null,
    resolution: null,
    jobId: "job-1",
    status: {
      jobId: "job-1",
      requestRevision: state.request.revision,
      state: "running",
      stage: "fitting_design",
      statusVersion: 2,
      queuedAtEpochSeconds: 1_900_000_000,
      startedAtEpochSeconds: 1_900_000_001,
      completedAtEpochSeconds: null,
      deadlineAtEpochSeconds: 1_900_000_060,
      resultIds: [],
      outcome: null,
      expiresAtEpochSeconds: 2_000_000_000,
      pollAfterSeconds: 1,
    },
    previousStatus: null,
    issue: null,
  };
  const markup = renderScreen("generating", state, savedRoutes, workflow);

  assert.match(markup, /class="stage-list"[^>]*role="list"[^>]*aria-label="Route generation progress"/);
  assert.match(markup, /aria-current="step"[^>]*>.*Fitting your design/);
  assert.doesNotMatch(markup, /stage-list[^>]*aria-live/);
});

test("responsive shell keeps controls reachable and honours reduced motion", async () => {
  const css = await readFile(new URL("../src/styles.css", import.meta.url), "utf8");

  assert.match(css, /@media\(max-width:700px\)[\s\S]*\.workspace[^}]*width:100%/);
  assert.match(css, /\.primary,.secondary,.danger\{[^}]*min-height:48px/);
  assert.match(css, /@media\(prefers-reduced-motion:reduce\)[\s\S]*animation:none!important/);
  assert.doesNotMatch(css, /html,body,#app\{[^}]*overflow-x:hidden/);
});

test("saved-route rename is a labelled dialog with field help", () => {
  const markup = renderScreen("saved", initialState(), {
    kind: "available",
    routes: [{
      id: "route-1",
      name: "Morning route",
      savedAtEpochMilliseconds: 1,
      storageLabel: "Saved on this device",
      request: initialState().request,
      snapshot: routeSnapshot(),
    }],
    errors: [],
  });

  assert.match(markup, /<dialog id="rename-route-dialog"[^>]*aria-labelledby="rename-route-title"/);
  assert.match(markup, /<label for="rename-route-name">Route name<\/label>/);
  assert.match(markup, /id="rename-route-name"[^>]*aria-describedby="rename-route-help"/);
});
