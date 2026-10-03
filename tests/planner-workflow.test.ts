import assert from "node:assert/strict";
import test from "node:test";

import {
  ACTIVE_JOB_STORAGE_KEY,
  PlannerWorkflow,
  type StoragePort,
} from "../src/client/planner-workflow.ts";
import { parseManualCoordinates } from "../src/domain/start-selection.ts";
import type { RouteRequest } from "../src/domain/planner.ts";

class MemoryStorage implements StoragePort {
  readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

const request: RouteRequest = {
  start: "28.60000, 77.20000",
  activity: "Walking",
  design: "Heart",
  distanceKm: 5,
  tolerancePercent: 10,
  returnsToStart: true,
  revision: 1,
};
const proposed = parseManualCoordinates(request.start)!;
const resolution = {
  id: "7bf80ccf-68e8-49ec-a3e8-cad667bb95a0",
  requested_point: { longitude: 77.2, latitude: 28.6 },
  candidate_network_point: { longitude: 77.2001, latitude: 28.6001 },
  snap_distance_metres: 15,
  profile_version: "walking-v1",
  coverage_policy_version: "coverage-v1",
  data_version: "map-v1",
  expires_at_epoch_seconds: 1_900_000_000,
  confirmation_required: true,
};
const jobId = "3f8b4f31-ee11-4f11-87aa-b4f15fc00cf1";

function json(value: unknown, status = 200, headers?: HeadersInit): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json", ...headers },
  });
}

function activeStatus(revision = 1) {
  return {
    job_id: jobId,
    request_revision: revision,
    state: "running",
    stage: "fitting_design",
    status_version: 3,
    queued_at_epoch_seconds: 1_800_000_000,
    started_at_epoch_seconds: 1_800_000_001,
    completed_at_epoch_seconds: null,
    deadline_at_epoch_seconds: 1_800_000_060,
    expires_at_epoch_seconds: 1_900_000_000,
    outcome: null,
    result_ids: [],
    poll_after_seconds: 1,
  };
}

test("a job cannot start before explicit confirmation and duplicate submission shares one logical request", async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  let resolveResponse: (response: Response) => void = () => undefined;
  const pendingResolution = new Promise<Response>((resolve) => { resolveResponse = resolve; });
  const fetcher: typeof fetch = async (url, init) => {
    calls.push({ url: String(url), init });
    if (String(url).endsWith("/starts/resolve")) return pendingResolution;
    if (String(url).includes("/confirm")) return json({ confirmed_start: resolution.candidate_network_point });
    if (String(url).endsWith("/route-jobs")) return json({ job_id: jobId, request_revision: 1 }, 202);
    throw new Error("unexpected request");
  };
  const workflow = new PlannerWorkflow({
    fetcher,
    storage: new MemoryStorage(),
    csrfToken: () => "csrf",
    newIdempotencyKey: () => "logical-request-key",
    currentRevision: () => request.revision,
    schedule: () => 1,
    cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  const one = workflow.submit(request, proposed);
  const two = workflow.submit(request, proposed);
  assert.equal(calls.length, 1);
  resolveResponse(json(resolution));
  await Promise.all([one, two]);
  assert.equal(workflow.snapshot.phase, "awaiting_confirmation");
  assert.equal(calls.filter((call) => call.url.endsWith("/route-jobs")).length, 0);

  await Promise.all([workflow.confirmAndStart(), workflow.confirmAndStart()]);
  assert.equal(calls.filter((call) => call.url.includes("/confirm")).length, 1);
  const submissions = calls.filter((call) => call.url.endsWith("/route-jobs"));
  assert.equal(submissions.length, 1);
  const submitted = JSON.parse(String(submissions[0]?.init?.body));
  assert.equal(submitted.idempotency_key, "logical-request-key");
  assert.equal(submitted.return_to_start, true);
  assert.equal(workflow.snapshot.phase, "polling");
});

test("invalid fields and unsupported coverage never create a job", async () => {
  const calls: string[] = [];
  const workflow = new PlannerWorkflow({
    fetcher: async (url) => {
      calls.push(String(url));
      return json({ outcome: { code: "unsupported_coverage" } }, 422);
    },
    storage: new MemoryStorage(), csrfToken: () => "csrf", currentRevision: () => 1,
    newIdempotencyKey: () => "key", schedule: () => 1, cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  await workflow.submit({ ...request, distanceKm: 0 }, proposed);
  assert.equal(workflow.snapshot.issue, "invalid_fields");
  assert.equal(calls.length, 0);
  await workflow.submit(request, proposed);
  assert.equal(workflow.snapshot.issue, "unsupported_coverage");
  assert.equal(calls.filter((url) => url.endsWith("/route-jobs")).length, 0);
});

test("refresh resumes an owned unexpired job with ETag polling and bounded backoff", async () => {
  const storage = new MemoryStorage();
  storage.setItem(ACTIVE_JOB_STORAGE_KEY, JSON.stringify({
    version: 1,
    jobId,
    request,
    proposedStart: proposed,
    expiresAtEpochSeconds: 1_900_000_000,
    etag: '"2"',
    statusPayload: activeStatus(),
  }));
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  const delays: number[] = [];
  const currentRevision = 1;
  const workflow = new PlannerWorkflow({
    fetcher: async (url, init) => {
      calls.push({ url: String(url), init });
      return calls.length === 1
        ? json(activeStatus(), 200, { etag: '"3"' })
        : new Response(null, { status: 304, headers: { etag: '"3"' } });
    },
    storage, csrfToken: () => "csrf", currentRevision: () => currentRevision,
    newIdempotencyKey: () => "key",
    schedule: (callback, delay) => { delays.push(delay); if (delays.length === 1) queueMicrotask(callback); return delays.length; },
    cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  assert.equal(await workflow.resume(), true);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(workflow.snapshot.request?.revision, 1);
  assert.equal(workflow.snapshot.status?.stage, "fitting_design");
  assert.equal(new Headers(calls[0]?.init?.headers).get("if-none-match"), '"2"');
  assert.deepEqual(delays.slice(0, 2), [1_000, 2_000]);
});

test("cancellation and request revisions prevent late results from replacing current work", async () => {
  const storage = new MemoryStorage();
  let currentRevision = 1;
  let releasePoll: (response: Response) => void = () => undefined;
  const latePoll = new Promise<Response>((resolve) => { releasePoll = resolve; });
  const scheduled: Array<() => void> = [];
  const fetcher: typeof fetch = async (url, init) => {
    const target = String(url);
    if (target.endsWith("/starts/resolve")) return json(resolution);
    if (target.includes("/confirm")) return json({ confirmed_start: resolution.candidate_network_point });
    if (target.endsWith("/route-jobs")) return json({ job_id: jobId, request_revision: 1 }, 202);
    if (init?.method === "DELETE") return json({ state: "cancelled" });
    if (target.includes(jobId)) return latePoll;
    throw new Error("unexpected request");
  };
  const workflow = new PlannerWorkflow({
    fetcher, storage, csrfToken: () => "csrf", currentRevision: () => currentRevision,
    newIdempotencyKey: () => "key",
    schedule: (callback) => { scheduled.push(callback); return scheduled.length; },
    cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  await workflow.submit(request, proposed);
  await workflow.confirmAndStart();
  scheduled.shift()?.();
  await Promise.resolve();
  await workflow.cancel();
  releasePoll(json({
    ...activeStatus(), state: "succeeded", stage: "comparing_routes",
    status_version: 4, completed_at_epoch_seconds: 1_800_000_050,
    outcome: { code: "succeeded", budget_exhausted: false, retryable: false, retry_after_seconds: null },
    result_ids: ["8bf80ccf-68e8-49ec-a3e8-cad667bb95a0"], poll_after_seconds: null,
  }));
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(workflow.snapshot.phase, "cancelled");
  assert.equal(workflow.snapshot.status, null);
  assert.equal(storage.getItem(ACTIVE_JOB_STORAGE_KEY), null);

  storage.setItem(ACTIVE_JOB_STORAGE_KEY, JSON.stringify({
    version: 1, jobId, request, proposedStart: proposed,
    expiresAtEpochSeconds: 1_900_000_000, etag: null,
  }));
  currentRevision = 2;
  const stale = new PlannerWorkflow({
    fetcher: async () => json({
      ...activeStatus(), state: "succeeded", stage: "comparing_routes",
      status_version: 4, completed_at_epoch_seconds: 1_800_000_050,
      outcome: { code: "succeeded", budget_exhausted: false, retryable: false, retry_after_seconds: null },
      result_ids: ["8bf80ccf-68e8-49ec-a3e8-cad667bb95a0"], poll_after_seconds: null,
    }),
    storage, csrfToken: () => "csrf", currentRevision: () => currentRevision,
    newIdempotencyKey: () => "key", schedule: () => 1, cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  await stale.resume();
  assert.equal(stale.snapshot.status, null);
  assert.equal(stale.snapshot.previousStatus?.state, "succeeded");
  assert.equal(stale.snapshot.phase, "stale");
});
