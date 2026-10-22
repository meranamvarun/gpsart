import assert from "node:assert/strict";
import test from "node:test";

import { renderScreen } from "../src/app/screen-controller.ts";
import { acceptCatalogueLoad, initialState, selectDesignCategory, updateRequest } from "../src/app/state.ts";
import type { PlannerWorkflowSnapshot } from "../src/client/planner-workflow.ts";

test("planner defaults to walking and disables unreviewed cycling", () => {
  const state = initialState();
  const markup = renderScreen("planner", state, { kind: "available", routes: [], errors: [] });

  assert.equal(state.request.activity, "Walking");
  assert.match(markup, /data-activity="Walking"[^>]*aria-pressed="true"/);
  assert.doesNotMatch(markup, /data-activity="Walking"[^>]*disabled/);
  assert.doesNotMatch(markup, /data-activity="Running"[^>]*disabled/);
  assert.match(markup, /data-activity="Cycling"[^>]*aria-pressed="false"[^>]*disabled[^>]*aria-describedby="cycling-availability"/);
  assert.match(markup, /id="cycling-availability"[^>]*>Cycling is not available in this pilot because its routing data has not been reviewed\.<\/p>/);

  const previousCyclingRequest = { ...state, request: { ...state.request, activity: "Cycling" as const } };
  const previousMarkup = renderScreen("planner", previousCyclingRequest, { kind: "available", routes: [], errors: [] });
  assert.match(previousMarkup, /data-activity="Cycling"[^>]*aria-pressed="true"[^>]*disabled/);
  assert.match(previousMarkup, /<button class="primary full" type="submit" disabled>.*Activity unavailable<\/button>/);
});

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

test("an unavailable guest session gives an actionable error", () => {
  const state = initialState();
  const workflow: PlannerWorkflowSnapshot = {
    phase: "error",
    request: { ...state.request, start: "0.50000, -150.25000", revision: 1 },
    proposedStart: {
      point: { latitude: 0.5, longitude: -150.25 },
      source: "manual",
      label: "0.50000, -150.25000",
    },
    resolution: null,
    jobId: null,
    status: null,
    previousStatus: null,
    issue: "csrf_unavailable",
  };

  const markup = renderScreen("error", state, { kind: "available", routes: [], errors: [] }, workflow);

  assert.match(markup, /private guest session is not ready/i);
  assert.match(markup, /reload/i);
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

test("catalogue category changes keep the active design visible and aligned with the CTA", () => {
  const loaded = acceptCatalogueLoad(initialState(), { kind: "available", designs: [
    {
      id: "heart", version: 1, status: "published", category: "shape", name: "Heart", accessibleName: "Heart route design",
      complexityHint: "simple", previewSvg: "<svg></svg>", definitionHash: "a".repeat(64),
    },
    {
      id: "HI", version: 1, status: "published", category: "text", name: "HI", accessibleName: "HI route design",
      complexityHint: "moderate", previewSvg: "<svg></svg>", definitionHash: "b".repeat(64),
    },
  ] });

  const textState = selectDesignCategory(loaded, "Text");
  const textMarkup = renderScreen("catalogue", textState, { kind: "available", routes: [], errors: [] });
  assert.equal(textState.request.design, "HI");
  assert.match(textMarkup, /data-design="HI"[^>]*aria-pressed="true"/);
  assert.match(textMarkup, />Use HI /);
  assert.doesNotMatch(textMarkup, /Use Heart/);

  const shapeState = selectDesignCategory(textState, "Shapes");
  const shapeMarkup = renderScreen("catalogue", shapeState, { kind: "available", routes: [], errors: [] });
  assert.equal(shapeState.request.design, "Heart");
  assert.match(shapeMarkup, /data-design="Heart"[^>]*aria-pressed="true"/);
  assert.match(shapeMarkup, />Use Heart /);
  assert.doesNotMatch(shapeMarkup, /Use HI/);

  const selectedTextState = updateRequest(textState, { design: "GO" });
  assert.equal(selectDesignCategory(selectedTextState, "Text").request.design, "GO");
});
