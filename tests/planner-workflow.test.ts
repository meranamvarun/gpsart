import assert from "node:assert/strict";
import test from "node:test";

import {
  ACTIVE_JOB_STORAGE_KEY,
  PENDING_CREATION_STORAGE_KEY,
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
  start: "0.50000, -150.25000",
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
  requested_point: { longitude: -150.25, latitude: 0.5 },
  candidate_network_point: { longitude: -150.2499, latitude: 0.5001 },
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
  const storage = new MemoryStorage();
  const workflow = new PlannerWorkflow({
    fetcher,
    storage,
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
  await workflow.submit(request, proposed);
  assert.equal(calls.filter((call) => call.url.endsWith("/starts/resolve")).length, 1);

  await Promise.all([workflow.confirmAndStart(), workflow.confirmAndStart()]);
  assert.equal(calls.filter((call) => call.url.includes("/confirm")).length, 1);
  const submissions = calls.filter((call) => call.url.endsWith("/route-jobs"));
  assert.equal(submissions.length, 1);
  const submitted = JSON.parse(String(submissions[0]?.init?.body));
  assert.equal(submitted.idempotency_key, "logical-request-key");
  assert.equal(submitted.return_to_start, true);
  assert.equal(workflow.snapshot.phase, "polling");
  const stored = JSON.parse(storage.getItem(ACTIVE_JOB_STORAGE_KEY)!);
  assert.equal(stored.expiresAtEpochSeconds, null, "start-resolution expiry is not the job expiry");
});

test("an uncertain creation retry recovers the exact confirmed request before newer work", async () => {
  const calls: Array<{ url: string; init?: RequestInit }> = [];
  let creations = 0;
  let currentRevision = 1;
  const workflow = new PlannerWorkflow({
    fetcher: async (url, init) => {
      const target = String(url);
      calls.push({ url: target, init });
      if (target.endsWith("/starts/resolve")) return json(resolution);
      if (target.includes("/confirm")) return json({ confirmed_start: resolution.candidate_network_point });
      if (target.endsWith("/route-jobs")) {
        creations += 1;
        if (creations === 1) throw new TypeError("response lost after acceptance");
        return json({ job_id: jobId, request_revision: 1 }, 202);
      }
      throw new Error("unexpected request");
    },
    storage: new MemoryStorage(), csrfToken: () => "csrf",
    currentRevision: () => currentRevision, newIdempotencyKey: () => "one-logical-key",
    schedule: () => 1, cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  await workflow.submit(request, proposed);
  await workflow.confirmAndStart();
  assert.equal(workflow.snapshot.issue, "network_unavailable");
  currentRevision = 2;
  const newerRequest = { ...request, start: "0.60000, -150.35000", revision: 2 };
  await workflow.submit(newerRequest, parseManualCoordinates(newerRequest.start)!);
  const resolveCalls = calls.filter((call) => call.url.endsWith("/starts/resolve"));
  const confirmCalls = calls.filter((call) => call.url.includes("/confirm"));
  const creationCalls = calls.filter((call) => call.url.endsWith("/route-jobs"));
  assert.equal(resolveCalls.length, 1);
  assert.equal(confirmCalls.length, 1);
  assert.equal(creationCalls.length, 2);
  assert.equal(creationCalls[0]?.init?.body, creationCalls[1]?.init?.body);
  assert.equal(JSON.parse(String(creationCalls[1]?.init?.body)).idempotency_key, "one-logical-key");
  assert.equal(workflow.snapshot.phase, "polling");
  assert.equal(workflow.snapshot.request?.revision, 1);
  assert.equal(workflow.snapshot.issue, "active_job");
});

test("refresh strictly restores and replays an uncertain creation before new work", async () => {
  const storage = new MemoryStorage();
  let firstBody: BodyInit | null | undefined;
  const first = new PlannerWorkflow({
    fetcher: async (url, init) => {
      const target = String(url);
      if (target.endsWith("/starts/resolve")) return json(resolution);
      if (target.includes("/confirm")) return json({ confirmed_start: resolution.candidate_network_point });
      if (target.endsWith("/route-jobs")) {
        firstBody = init?.body;
        throw new TypeError("response lost after acceptance");
      }
      throw new Error("unexpected request");
    },
    storage, csrfToken: () => "csrf", currentRevision: () => 1,
    newIdempotencyKey: () => "refresh-safe-key", schedule: () => 1,
    cancelSchedule: () => undefined, now: () => 1_800_000_000_000,
  });
  await first.submit(request, proposed);
  await first.confirmAndStart();
  assert.equal(first.snapshot.issue, "network_unavailable");
  assert.ok(storage.getItem(PENDING_CREATION_STORAGE_KEY));

  const replayed: Array<{ url: string; body: BodyInit | null | undefined }> = [];
  const refreshed = new PlannerWorkflow({
    fetcher: async (url, init) => {
      replayed.push({ url: String(url), body: init?.body });
      return json({ job_id: jobId, request_revision: 1 }, 202);
    },
    storage, csrfToken: () => "csrf", currentRevision: () => 2,
    newIdempotencyKey: () => { throw new Error("refresh must retain the original key"); },
    schedule: () => 1, cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  assert.equal(await refreshed.resume(), true);
  assert.deepEqual(replayed.map((call) => call.url), ["/api/v1/route-jobs"]);
  assert.equal(replayed[0]?.body, firstBody);
  assert.equal(refreshed.snapshot.request?.revision, 1);
  assert.equal(refreshed.snapshot.issue, "active_job");
  assert.equal(storage.getItem(PENDING_CREATION_STORAGE_KEY), null);
  assert.ok(storage.getItem(ACTIVE_JOB_STORAGE_KEY));
});

test("refresh rejects a tampered uncertain creation instead of replaying it", async () => {
  const storage = new MemoryStorage();
  const first = new PlannerWorkflow({
    fetcher: async (url) => {
      const target = String(url);
      if (target.endsWith("/starts/resolve")) return json(resolution);
      if (target.includes("/confirm")) return json({ confirmed_start: resolution.candidate_network_point });
      throw new TypeError("response lost after acceptance");
    },
    storage, csrfToken: () => "csrf", currentRevision: () => 1,
    newIdempotencyKey: () => "tamper-key", schedule: () => 1,
    cancelSchedule: () => undefined, now: () => 1_800_000_000_000,
  });
  await first.submit(request, proposed);
  await first.confirmAndStart();
  const stored = JSON.parse(storage.getItem(PENDING_CREATION_STORAGE_KEY)!);
  stored.body = JSON.stringify({ ...JSON.parse(stored.body), resolved_start_id: jobId });
  storage.setItem(PENDING_CREATION_STORAGE_KEY, JSON.stringify(stored));

  let fetched = false;
  const refreshed = new PlannerWorkflow({
    fetcher: async () => { fetched = true; throw new Error("must not replay tampered input"); },
    storage, csrfToken: () => "csrf", currentRevision: () => 1,
    newIdempotencyKey: () => "new-key", schedule: () => 1,
    cancelSchedule: () => undefined, now: () => 1_800_000_000_000,
  });
  assert.equal(await refreshed.resume(), false);
  assert.equal(fetched, false);
  assert.equal(storage.getItem(PENDING_CREATION_STORAGE_KEY), null);
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

test("a route edit invalidates an older pending start confirmation", async () => {
  const calls: string[] = [];
  let currentRevision = 1;
  const workflow = new PlannerWorkflow({
    fetcher: async (url) => {
      calls.push(String(url));
      if (String(url).endsWith("/starts/resolve")) return json(resolution);
      throw new Error("stale confirmation must not reach the server");
    },
    storage: new MemoryStorage(), csrfToken: () => "csrf",
    currentRevision: () => currentRevision, newIdempotencyKey: () => "key",
    schedule: () => 1, cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  await workflow.submit(request, proposed);
  currentRevision = 2;
  await workflow.confirmAndStart();
  assert.equal(workflow.snapshot.phase, "stale");
  assert.deepEqual(calls, ["/api/v1/starts/resolve"]);
});

test("a late start resolution cannot overwrite a newer logical request", async () => {
  let releaseOlder: (response: Response) => void = () => undefined;
  const olderResponse = new Promise<Response>((resolve) => { releaseOlder = resolve; });
  const newerRequest = { ...request, start: "0.60000, -150.35000", revision: 2 };
  const newerStart = parseManualCoordinates(newerRequest.start)!;
  const newerResolution = {
    ...resolution,
    id: "8bf80ccf-68e8-49ec-a3e8-cad667bb95a0",
    requested_point: newerStart.point,
    candidate_network_point: { latitude: 0.6001, longitude: -150.3499 },
  };
  let currentRevision = 1;
  let resolutions = 0;
  const workflow = new PlannerWorkflow({
    fetcher: async (url) => {
      if (!String(url).endsWith("/starts/resolve")) throw new Error("unexpected request");
      resolutions += 1;
      return resolutions === 1 ? olderResponse : json(newerResolution);
    },
    storage: new MemoryStorage(), csrfToken: () => "csrf",
    currentRevision: () => currentRevision, newIdempotencyKey: () => `key-${resolutions}`,
    schedule: () => 1, cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  const older = workflow.submit(request, proposed);
  currentRevision = 2;
  await workflow.submit(newerRequest, newerStart);
  releaseOlder(json(resolution));
  await older;
  assert.equal(workflow.snapshot.phase, "awaiting_confirmation");
  assert.equal(workflow.snapshot.request?.revision, 2);
  assert.equal(workflow.snapshot.resolution?.id, newerResolution.id);
});

test("an edit while confirmation is in flight cannot create or install the older job", async () => {
  let releaseConfirmation: (response: Response) => void = () => undefined;
  const pendingConfirmation = new Promise<Response>((resolve) => { releaseConfirmation = resolve; });
  const calls: string[] = [];
  let currentRevision = 1;
  const workflow = new PlannerWorkflow({
    fetcher: async (url) => {
      calls.push(String(url));
      if (String(url).endsWith("/starts/resolve")) return json(resolution);
      if (String(url).includes("/confirm")) return pendingConfirmation;
      throw new Error("stale confirmation must not create a job");
    },
    storage: new MemoryStorage(), csrfToken: () => "csrf",
    currentRevision: () => currentRevision, newIdempotencyKey: () => "key",
    schedule: () => 1, cancelSchedule: () => undefined,
    now: () => 1_800_000_000_000,
  });
  await workflow.submit(request, proposed);
  const confirming = workflow.confirmAndStart();
  currentRevision = 2;
  releaseConfirmation(json({ confirmed_start: resolution.candidate_network_point }));
  await confirming;
  assert.equal(workflow.snapshot.phase, "stale");
  assert.equal(calls.filter((url) => url.endsWith("/route-jobs")).length, 0);
});

test("a job accepted after its revision becomes stale is tracked until its outcome is retained", async () => {
  let releaseCreation: (response: Response) => void = () => undefined;
  const pendingCreation = new Promise<Response>((resolve) => { releaseCreation = resolve; });
  const storage = new MemoryStorage();
  const scheduled: Array<() => void> = [];
  const calls: string[] = [];
  let currentRevision = 1;
  const workflow = new PlannerWorkflow({
    fetcher: async (url) => {
      const target = String(url);
      calls.push(target);
      if (target.endsWith("/starts/resolve")) return json(resolution);
      if (target.includes("/confirm")) return json({ confirmed_start: resolution.candidate_network_point });
      if (target.endsWith("/route-jobs")) return pendingCreation;
      return json({
        ...activeStatus(), state: "succeeded", stage: "comparing_routes",
        status_version: 4, completed_at_epoch_seconds: 1_800_000_050,
        outcome: { code: "succeeded", budget_exhausted: false, retryable: false, retry_after_seconds: null },
        result_ids: ["8bf80ccf-68e8-49ec-a3e8-cad667bb95a0"], poll_after_seconds: null,
      });
    },
    storage, csrfToken: () => "csrf", currentRevision: () => currentRevision,
    newIdempotencyKey: () => "key", schedule: (callback) => { scheduled.push(callback); return scheduled.length; },
    cancelSchedule: () => undefined, now: () => 1_800_000_000_000,
  });
  await workflow.submit(request, proposed);
  const creation = workflow.confirmAndStart();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(calls.filter((url) => url.endsWith("/route-jobs")).length, 1);
  currentRevision = 2;
  releaseCreation(json({ job_id: jobId, request_revision: 1 }, 202));
  await creation;
  assert.equal(workflow.snapshot.phase, "polling");
  assert.equal(workflow.snapshot.issue, "active_job");
  assert.equal(JSON.parse(storage.getItem(ACTIVE_JOB_STORAGE_KEY)!).jobId, jobId);
  scheduled.shift()?.();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(workflow.snapshot.phase, "stale");
  assert.equal(workflow.snapshot.previousStatus?.state, "succeeded");
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

test("refresh polls a newly accepted job before its authoritative job expiry is known", async () => {
  const storage = new MemoryStorage();
  storage.setItem(ACTIVE_JOB_STORAGE_KEY, JSON.stringify({
    version: 1, jobId, request, proposedStart: proposed,
    expiresAtEpochSeconds: null, etag: null, statusPayload: null,
  }));
  let polls = 0;
  const workflow = new PlannerWorkflow({
    fetcher: async () => { polls += 1; return json(activeStatus()); },
    storage, csrfToken: () => "csrf", currentRevision: () => 1,
    newIdempotencyKey: () => "key", schedule: () => 1,
    cancelSchedule: () => undefined, now: () => 1_800_000_000_000,
  });
  assert.equal(await workflow.resume(), true);
  assert.equal(polls, 1);
  const refreshed = JSON.parse(storage.getItem(ACTIVE_JOB_STORAGE_KEY)!);
  assert.equal(refreshed.expiresAtEpochSeconds, activeStatus().expires_at_epoch_seconds);
});

test("refresh rejects tampered request and start discriminants before rendering or polling", async () => {
  for (const corruption of [
    { request: { ...request, activity: "<img src=x onerror=alert(1)>" } },
    { proposedStart: { ...proposed, source: "unknown" } },
  ]) {
    const storage = new MemoryStorage();
    storage.setItem(ACTIVE_JOB_STORAGE_KEY, JSON.stringify({
      version: 1, jobId, request, proposedStart: proposed,
      expiresAtEpochSeconds: 1_900_000_000, etag: null,
      ...corruption,
    }));
    let fetched = false;
    const workflow = new PlannerWorkflow({
      fetcher: async () => { fetched = true; throw new Error("must not poll"); },
      storage, csrfToken: () => "csrf", currentRevision: () => 1,
      newIdempotencyKey: () => "key", schedule: () => 1,
      cancelSchedule: () => undefined, now: () => 1_800_000_000_000,
    });
    assert.equal(await workflow.resume(), false);
    assert.equal(fetched, false);
    assert.equal(storage.getItem(ACTIVE_JOB_STORAGE_KEY), null);
  }
});

test("a newer submission cannot abandon an active older revision before its outcome is retained", async () => {
  const storage = new MemoryStorage();
  let currentRevision = 1;
  const scheduled: Array<() => void> = [];
  const calls: string[] = [];
  const workflow = new PlannerWorkflow({
    fetcher: async (url) => {
      const target = String(url);
      calls.push(target);
      if (target.endsWith("/starts/resolve")) return json(resolution);
      if (target.includes("/confirm")) return json({ confirmed_start: resolution.candidate_network_point });
      if (target.endsWith("/route-jobs")) return json({ job_id: jobId, request_revision: 1 }, 202);
      return json({
        ...activeStatus(), state: "succeeded", stage: "comparing_routes",
        status_version: 4, completed_at_epoch_seconds: 1_800_000_050,
        outcome: { code: "succeeded", budget_exhausted: false, retryable: false, retry_after_seconds: null },
        result_ids: ["8bf80ccf-68e8-49ec-a3e8-cad667bb95a0"], poll_after_seconds: null,
      });
    },
    storage, csrfToken: () => "csrf", currentRevision: () => currentRevision,
    newIdempotencyKey: () => "key", schedule: (callback) => { scheduled.push(callback); return scheduled.length; },
    cancelSchedule: () => undefined, now: () => 1_800_000_000_000,
  });
  await workflow.submit(request, proposed);
  await workflow.confirmAndStart();
  currentRevision = 2;
  const newerRequest = { ...request, start: "0.60000, -150.35000", revision: 2 };
  await workflow.submit(newerRequest, parseManualCoordinates(newerRequest.start)!);
  assert.equal(workflow.snapshot.phase, "polling");
  assert.equal(workflow.snapshot.issue, "active_job");
  assert.equal(calls.filter((url) => url.endsWith("/starts/resolve")).length, 1);
  scheduled.shift()?.();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(workflow.snapshot.phase, "stale");
  assert.equal(workflow.snapshot.previousStatus?.state, "succeeded");
  assert.equal(storage.getItem(ACTIVE_JOB_STORAGE_KEY), null);
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
