import assert from "node:assert/strict";
import test from "node:test";

import { outcomePresentation, screenForWorkflow } from "../src/app/outcome-flow.ts";
import { renderScreen } from "../src/app/screen-controller.ts";
import {
  initialState,
  recordCompletedAttempt,
  retryCompletedAttempt,
  updateRequest,
} from "../src/app/state.ts";
import type { PlannerWorkflowSnapshot } from "../src/client/planner-workflow.ts";
import type { RouteJobOutcomeCode, RouteJobState, RouteJobStatus } from "../src/domain/job-status.ts";
import type { RouteRequest } from "../src/domain/planner.ts";

const originalRequest: RouteRequest = {
  start: "12.97160, 77.59460",
  activity: "Cycling",
  design: "Heart",
  distanceKm: 15,
  tolerancePercent: 10,
  returnsToStart: true,
  revision: 3,
};

function terminalStatus(code: RouteJobOutcomeCode): RouteJobStatus {
  const state: RouteJobState = code === "succeeded" ? "succeeded"
    : code === "cancelled" ? "cancelled"
      : code === "timed_out" ? "timed_out"
        : ["provider_unavailable", "attempts_exhausted", "legacy_unknown"].includes(code) ? "failed"
          : "no_match";
  return {
    jobId: "3f8b4f31-ee11-4f11-87aa-b4f15fc00cf1",
    requestRevision: originalRequest.revision,
    state,
    stage: "comparing_routes",
    statusVersion: 4,
    queuedAtEpochSeconds: 1_800_000_000,
    startedAtEpochSeconds: 1_800_000_001,
    completedAtEpochSeconds: 1_800_000_050,
    deadlineAtEpochSeconds: 1_800_000_060,
    expiresAtEpochSeconds: 1_900_000_000,
    outcome: {
      code,
      budgetExhausted: code === "exhausted_budget" || code === "timed_out",
      retryable: code === "provider_unavailable",
      retryAfterSeconds: code === "provider_unavailable" ? 30 : null,
    },
    resultIds: code === "succeeded" ? ["8bf80ccf-68e8-49ec-a3e8-cad667bb95a0"] : [],
    pollAfterSeconds: null,
  };
}

function terminalWorkflow(code: RouteJobOutcomeCode): PlannerWorkflowSnapshot {
  const status = terminalStatus(code);
  return {
    phase: "terminal",
    request: originalRequest,
    proposedStart: {
      source: "manual",
      label: originalRequest.start,
      point: { latitude: 12.9716, longitude: 77.5946 },
    },
    resolution: null,
    jobId: null,
    status,
    previousStatus: null,
    issue: null,
  };
}

test("every classified terminal outcome selects a truthful destination and advice", () => {
  const expected: Readonly<Record<RouteJobOutcomeCode, string>> = {
    succeeded: "results",
    unsupported_coverage: "coverage",
    no_routable_access: "coverage",
    disconnected_network: "no-match",
    insufficient_visual_match: "no-match",
    outside_distance: "no-match",
    exhausted_budget: "no-match",
    provider_unavailable: "error",
    attempts_exhausted: "error",
    cancelled: "planner",
    timed_out: "error",
    legacy_unknown: "error",
  };
  for (const [code, screen] of Object.entries(expected) as Array<[RouteJobOutcomeCode, string]>) {
    assert.equal(screenForWorkflow(terminalWorkflow(code)), screen, code);
    const presentation = outcomePresentation(code);
    assert.ok(presentation.explanation.length > 20, code);
    assert.doesNotMatch(`${presentation.title} ${presentation.explanation}`, /impossible|guaranteed|will fit/i, code);
    if (presentation.suggestions.length) {
      assert.equal(presentation.feasibility, "not_evaluated", code);
    }
    if (["coverage", "no-match", "error"].includes(screen)) {
      const completed = recordCompletedAttempt(
        { ...initialState(), request: originalRequest },
        originalRequest,
        terminalStatus(code),
      );
      const markup = renderScreen(screen as "coverage" | "no-match" | "error", completed, { kind: "available", snapshots: [] });
      assert.match(markup, new RegExp(presentation.title.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")), code);
      assert.match(markup, /not been checked for local feasibility/i, code);
    }
  }
});

test("start-resolution and dependency errors select coverage or error without hiding field errors", () => {
  const base: PlannerWorkflowSnapshot = {
    ...terminalWorkflow("provider_unavailable"),
    phase: "error",
    status: null,
  };
  assert.equal(screenForWorkflow({ ...base, issue: "unsupported_coverage" }), "coverage");
  assert.equal(screenForWorkflow({ ...base, issue: "no_routable_access" }), "coverage");
  assert.equal(screenForWorkflow({ ...base, issue: "network_unavailable" }), "error");
  assert.equal(screenForWorkflow({ ...base, issue: "invalid_fields" }), "planner");

  const prior = recordCompletedAttempt(
    { ...initialState(), request: originalRequest },
    originalRequest,
    terminalStatus("provider_unavailable"),
  );
  const currentFailure = {
    ...base,
    request: { ...originalRequest, revision: 4 },
    issue: "network_unavailable" as const,
  };
  const markup = renderScreen("error", prior, { kind: "available", snapshots: [] }, currentFailure);
  assert.match(markup, /could not complete this request/i);
  assert.doesNotMatch(markup, /data-action="retry-job"/);
});

test("refinement retains the completed request and result while advancing only the current revision", () => {
  const initial = { ...initialState(), request: originalRequest };
  const completed = recordCompletedAttempt(initial, originalRequest, terminalStatus("outside_distance"));
  const refined = updateRequest(completed, { distanceKm: 18, tolerancePercent: 20 });
  assert.equal(refined.request.revision, 4);
  assert.equal(refined.request.distanceKm, 18);
  assert.equal(refined.lastAttempt?.request.distanceKm, 15);
  assert.equal(refined.lastAttempt?.status.outcome?.code, "outside_distance");

  const failureMarkup = renderScreen("no-match", refined, { kind: "available", snapshots: [] });
  assert.match(failureMarkup, /15\.0 km/);
  assert.match(failureMarkup, /12\.97160, 77\.59460/);
  assert.doesNotMatch(failureMarkup, /18\.0 km/);

  const plannerMarkup = renderScreen("planner", refined, { kind: "available", snapshots: [] });
  assert.match(plannerMarkup, /18/);
  assert.match(plannerMarkup, /Previous search/i);
  assert.match(plannerMarkup, /15\.0 km/);
});

test("retrying a retryable completed dependency failure creates one new revision and preserves evidence", () => {
  const completed = recordCompletedAttempt(
    { ...initialState(), request: originalRequest },
    originalRequest,
    terminalStatus("provider_unavailable"),
  );
  const retried = retryCompletedAttempt(completed);
  assert.ok(retried);
  assert.equal(retried.request.revision, 4);
  assert.deepEqual({ ...retried.request, revision: 3 }, originalRequest);
  assert.equal(retried.lastAttempt?.status.jobId, terminalStatus("provider_unavailable").jobId);

  const errorMarkup = renderScreen("error", completed, { kind: "available", snapshots: [] });
  assert.match(errorMarkup, /data-action="retry-job"/);
  assert.match(errorMarkup, /new request revision/i);

  const nonRetryable = recordCompletedAttempt(
    { ...initialState(), request: originalRequest },
    originalRequest,
    terminalStatus("attempts_exhausted"),
  );
  assert.equal(retryCompletedAttempt(nonRetryable), null);
  assert.doesNotMatch(renderScreen("error", nonRetryable, { kind: "available", snapshots: [] }), /data-action="retry-job"/);

  const edited = updateRequest(completed, { design: "HI" });
  assert.equal(retryCompletedAttempt(edited), null, "retry never overwrites a newer user refinement");
  assert.doesNotMatch(
    renderScreen("error", edited, { kind: "available", snapshots: [] }),
    /data-action="retry-job"/,
    "a newer edit visibly disables retry of the retained attempt",
  );
});

test("refining after success keeps the successful request and observed result reference visible", () => {
  const completed = recordCompletedAttempt(
    { ...initialState(), request: originalRequest },
    originalRequest,
    terminalStatus("succeeded"),
  );
  const refined = updateRequest(completed, { design: "HI" });
  const resultMarkup = renderScreen("results", refined, { kind: "available", snapshots: [] });
  assert.match(resultMarkup, /Heart · Cycling · 15\.0 km/);
  assert.doesNotMatch(resultMarkup, /HI · Cycling/);
  assert.match(resultMarkup, /1 validated route reference was returned/i);
  assert.match(renderScreen("planner", refined, { kind: "available", snapshots: [] }), /Previous search · revision 3/i);
});
