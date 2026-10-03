import assert from "node:assert/strict";
import test from "node:test";

import { decodeRouteJobStatus } from "../src/domain/job-status.ts";

const jobId = "7bf80ccf-68e8-49ec-a3e8-cad667bb95a0";
const routeId = "3f8b4f31-ee11-4f11-87aa-b4f15fc00cf1";
const base = {
  job_id: jobId,
  request_revision: 3,
  state: "running",
  stage: "fitting_design",
  status_version: 4,
  queued_at_epoch_seconds: 1_800_000_000,
  started_at_epoch_seconds: 1_800_000_001,
  completed_at_epoch_seconds: null,
  deadline_at_epoch_seconds: 1_800_000_060,
  expires_at_epoch_seconds: 1_800_086_400,
  outcome: null,
  result_ids: [],
  poll_after_seconds: 1,
};

test("active job status decodes without inventing progress", () => {
  assert.deepEqual(decodeRouteJobStatus(base), {
    jobId,
    requestRevision: 3,
    state: "running",
    stage: "fitting_design",
    statusVersion: 4,
    queuedAtEpochSeconds: 1_800_000_000,
    startedAtEpochSeconds: 1_800_000_001,
    completedAtEpochSeconds: null,
    deadlineAtEpochSeconds: 1_800_000_060,
    expiresAtEpochSeconds: 1_800_086_400,
    outcome: null,
    resultIds: [],
    pollAfterSeconds: 1,
  });
  assert.equal(decodeRouteJobStatus({ ...base, percentage: 50 }), undefined);
  assert.equal(decodeRouteJobStatus({ ...base, stage: "making_progress" }), undefined);
});

test("terminal states require their matching evidence-backed outcome", () => {
  const succeeded = {
    ...base,
    state: "succeeded",
    stage: "comparing_routes",
    status_version: 8,
    completed_at_epoch_seconds: 1_800_000_055,
    outcome: { code: "succeeded", budget_exhausted: true, retryable: false, retry_after_seconds: null },
    result_ids: [routeId],
    poll_after_seconds: null,
  };
  assert.equal(decodeRouteJobStatus(succeeded)?.outcome?.code, "succeeded");
  assert.equal(decodeRouteJobStatus({ ...succeeded, result_ids: [] }), undefined);
  assert.equal(decodeRouteJobStatus({ ...succeeded, outcome: { ...succeeded.outcome, code: "timed_out" } }), undefined);

  for (const [state, code] of [
    ["no_match", "unsupported_coverage"],
    ["no_match", "no_routable_access"],
    ["no_match", "disconnected_network"],
    ["no_match", "insufficient_visual_match"],
    ["no_match", "outside_distance"],
    ["no_match", "exhausted_budget"],
    ["failed", "provider_unavailable"],
    ["failed", "attempts_exhausted"],
    ["cancelled", "cancelled"],
    ["timed_out", "timed_out"],
  ] as const) {
    const decoded = decodeRouteJobStatus({
      ...base,
      state,
      completed_at_epoch_seconds: 1_800_000_055,
      outcome: {
        code,
        budget_exhausted: code === "exhausted_budget" || code === "timed_out",
        retryable: code === "provider_unavailable",
        retry_after_seconds: code === "provider_unavailable" ? 30 : null,
      },
      poll_after_seconds: null,
    });
    assert.equal(decoded?.outcome?.code, code);
  }
});

test("timestamps, UUIDs, revisions, and retry guidance are strict", () => {
  assert.equal(decodeRouteJobStatus({ ...base, job_id: "------------------------------------" }), undefined);
  assert.equal(decodeRouteJobStatus({ ...base, status_version: 0 }), undefined);
  assert.equal(decodeRouteJobStatus({ ...base, completed_at_epoch_seconds: 1_800_000_002 }), undefined);
  assert.equal(decodeRouteJobStatus({ ...base, outcome: { code: ["timed_out"] } }), undefined);
  assert.equal(decodeRouteJobStatus({ ...base, invented: true }), undefined);
});
