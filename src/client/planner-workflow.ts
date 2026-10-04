import { decodeRouteJobStatus, type RouteJobStatus } from "../domain/job-status.ts";
import type { RouteRequest } from "../domain/planner.ts";
import { decodeAcceptedRouteJob, decodeRouteJobError, type RouteJobErrorCode } from "../domain/route-job.ts";
import type { ProposedStart } from "../domain/start-selection.ts";
import {
  decodeConfirmedStart,
  decodeStartOutcome,
  decodeStartResolution,
  type ProfileVersion,
  type StartOutcomeCode,
  type StartResolution,
} from "../domain/start-resolution.ts";

export const ACTIVE_JOB_STORAGE_KEY = "gpsart.active-job.v1";
const MIN_POLL_MILLISECONDS = 1_000;
const MAX_POLL_MILLISECONDS = 8_000;

export interface StoragePort {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export type WorkflowPhase =
  | "idle"
  | "resolving"
  | "awaiting_confirmation"
  | "submitting"
  | "polling"
  | "cancelled"
  | "terminal"
  | "stale"
  | "error";

export type WorkflowIssue = StartOutcomeCode | RouteJobErrorCode
  | "invalid_fields"
  | "active_job"
  | "invalid_response"
  | "network_unavailable"
  | "csrf_unavailable"
  | "expired_job";

export type PlannerWorkflowSnapshot = Readonly<{
  phase: WorkflowPhase;
  request: RouteRequest | null;
  proposedStart: ProposedStart | null;
  resolution: StartResolution | null;
  jobId: string | null;
  status: RouteJobStatus | null;
  previousStatus: RouteJobStatus | null;
  issue: WorkflowIssue | null;
}>;

type Schedule = (callback: () => void, delayMilliseconds: number) => unknown;
type CancelSchedule = (handle: unknown) => void;

type Dependencies = Readonly<{
  fetcher?: typeof fetch;
  storage?: StoragePort;
  csrfToken: () => string | null;
  currentRevision: () => number;
  newIdempotencyKey?: () => string;
  schedule?: Schedule;
  cancelSchedule?: CancelSchedule;
  now?: () => number;
  onChange?: (snapshot: PlannerWorkflowSnapshot) => void;
}>;

type ActiveJob = {
  version: 1;
  jobId: string;
  request: RouteRequest;
  proposedStart: ProposedStart;
  expiresAtEpochSeconds: number | null;
  etag: string | null;
  statusPayload: unknown | null;
};

type PendingCreation = {
  request: RouteRequest;
  proposedStart: ProposedStart;
  resolution: StartResolution;
  key: string;
  body: string;
};

const profileVersions: Readonly<Record<RouteRequest["activity"], ProfileVersion>> = {
  Walking: "walking-v1",
  Running: "running-v1",
  Cycling: "cycling-v1",
};

const templateIdentities: Readonly<Record<string, { id: string; version: string }>> = {
  Heart: { id: "heart", version: "1" },
  HI: { id: "HI", version: "1" },
};

export class PlannerWorkflow {
  private readonly fetcher: typeof fetch;
  private readonly storage: StoragePort;
  private readonly csrfToken: () => string | null;
  private readonly currentRevision: () => number;
  private readonly newIdempotencyKey: () => string;
  private readonly schedule: Schedule;
  private readonly cancelSchedule: CancelSchedule;
  private readonly now: () => number;
  private readonly onChange: (snapshot: PlannerWorkflowSnapshot) => void;
  private value: PlannerWorkflowSnapshot = emptySnapshot();
  private active: ActiveJob | null = null;
  private timer: unknown;
  private pollDelay = MIN_POLL_MILLISECONDS;
  private generation = 0;
  private submitFingerprint: string | null = null;
  private submissionKey: string | null = null;
  private submitting: Promise<void> | null = null;
  private confirming: Promise<void> | null = null;
  private pendingCreation: PendingCreation | null = null;

  constructor(dependencies: Dependencies) {
    this.fetcher = dependencies.fetcher ?? globalThis.fetch;
    this.storage = dependencies.storage ?? globalThis.sessionStorage;
    this.csrfToken = dependencies.csrfToken;
    this.currentRevision = dependencies.currentRevision;
    this.newIdempotencyKey = dependencies.newIdempotencyKey ?? (() => globalThis.crypto.randomUUID());
    this.schedule = dependencies.schedule ?? ((callback, delay) => globalThis.setTimeout(callback, delay));
    this.cancelSchedule = dependencies.cancelSchedule ?? ((handle) => globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>));
    this.now = dependencies.now ?? Date.now;
    this.onChange = dependencies.onChange ?? (() => undefined);
  }

  get snapshot(): PlannerWorkflowSnapshot { return this.value; }

  submit(request: RouteRequest, proposedStart: ProposedStart): Promise<void> {
    if (this.pendingCreation) {
      if (this.confirming) return this.confirming;
      if (this.submitting) return this.submitting;
      return this.retryPendingCreation();
    }
    const fingerprint = JSON.stringify({ request, proposedStart });
    if (this.submitting && this.submitFingerprint === fingerprint) return this.submitting;
    if (this.submitFingerprint === fingerprint
      && ["awaiting_confirmation", "submitting", "polling"].includes(this.value.phase)) {
      return Promise.resolve();
    }
    if (this.active) {
      this.update({ ...this.value, issue: "active_job" });
      return Promise.resolve();
    }
    if (!validRequest(request) || !validProposedStart(proposedStart) || request.start !== proposedStart.label) {
      this.update({ ...emptySnapshot(), request, proposedStart, phase: "error", issue: "invalid_fields" });
      return Promise.resolve();
    }
    this.stopTimer();
    const token = ++this.generation;
    this.confirming = null;
    if (this.submitFingerprint !== fingerprint || !this.submissionKey) {
      this.submissionKey = this.newIdempotencyKey();
    }
    this.submitFingerprint = fingerprint;
    this.update({ ...emptySnapshot(), request, proposedStart, phase: "resolving" });
    const operation = this.resolveStart(request, proposedStart, token)
      .finally(() => {
        if (this.submitting === operation) this.submitting = null;
      });
    this.submitting = operation;
    return operation;
  }

  confirmAndStart(): Promise<void> {
    if (this.confirming) return this.confirming;
    if (this.pendingCreation) {
      if (this.submitting) return this.submitting;
      return this.retryPendingCreation();
    }
    if (this.value.phase === "polling" || this.value.phase === "terminal") return Promise.resolve();
    const { request, proposedStart, resolution } = this.value;
    if (!request || !proposedStart || !resolution || !this.submissionKey) {
      this.update({ ...this.value, phase: "error", issue: "invalid_fields" });
      return Promise.resolve();
    }
    if (request.revision !== this.currentRevision()) {
      this.update({ ...this.value, phase: "stale", resolution: null, issue: null });
      return Promise.resolve();
    }
    const token = this.generation;
    this.update({ ...this.value, phase: "submitting", issue: null });
    const operation = this.confirmThenCreate(request, proposedStart, resolution, this.submissionKey, token)
      .finally(() => {
        if (this.confirming === operation) this.confirming = null;
      });
    this.confirming = operation;
    return operation;
  }

  async resume(): Promise<boolean> {
    const active = loadActive(this.storage);
    if (!active || active.expiresAtEpochSeconds !== null
      && active.expiresAtEpochSeconds <= Math.floor(this.now() / 1_000)) {
      this.removeActive();
      return false;
    }
    this.stopTimer();
    const token = ++this.generation;
    this.active = active;
    const storedStatus = active.statusPayload === null ? null : decodeRouteJobStatus(active.statusPayload) ?? null;
    this.update({
      ...emptySnapshot(),
      phase: "polling",
      request: active.request,
      proposedStart: active.proposedStart,
      jobId: active.jobId,
      status: storedStatus,
    });
    await this.poll(token);
    return true;
  }

  async cancel(): Promise<void> {
    const active = this.active;
    if (!active) return;
    this.stopTimer();
    this.generation += 1;
    const csrf = this.csrfToken();
    if (!csrf) {
      this.update({ ...this.value, phase: "error", issue: "csrf_unavailable" });
      return;
    }
    let response: Response;
    try {
      response = await this.fetcher(`/api/v1/route-jobs/${active.jobId}`, {
        method: "DELETE",
        credentials: "include",
        headers: { "X-CSRF-Token": csrf },
      });
    } catch {
      this.update({ ...this.value, phase: "error", issue: "network_unavailable" });
      return;
    }
    const payload = await responseJson(response);
    const terminalState = decodeCancellation(payload);
    if (!response.ok || !terminalState) {
      this.update({ ...this.value, phase: "error", issue: decodeRouteJobError(payload) ?? "invalid_response" });
      return;
    }
    if (terminalState !== "cancelled") {
      this.active = active;
      const token = ++this.generation;
      await this.poll(token);
      return;
    }
    this.removeActive();
    this.resetLogicalSubmission();
    this.update({ ...this.value, phase: "cancelled", jobId: null, status: null, issue: null });
  }

  dispose(): void {
    this.stopTimer();
    this.generation += 1;
  }

  private async resolveStart(request: RouteRequest, proposedStart: ProposedStart, token: number): Promise<void> {
    if (!this.fenceAsyncStep(token, request.revision)) return;
    const csrf = this.csrfToken();
    if (!csrf) {
      this.update({ ...this.value, phase: "error", issue: "csrf_unavailable" });
      return;
    }
    let response: Response;
    try {
      response = await this.fetcher("/api/v1/starts/resolve", {
        method: "POST",
        credentials: "include",
        headers: jsonHeaders(csrf),
        body: JSON.stringify({
          requested_point: proposedStart.point,
          profile: profile(request.activity),
        }),
      });
    } catch {
      if (this.fenceAsyncStep(token, request.revision)) {
        this.update({ ...this.value, phase: "error", issue: "network_unavailable" });
      }
      return;
    }
    const payload = await responseJson(response);
    if (!this.fenceAsyncStep(token, request.revision)) return;
    if (!response.ok) {
      this.update({ ...this.value, phase: "error", issue: decodeStartOutcome(payload) ?? "invalid_response" });
      return;
    }
    const resolution = decodeStartResolution(payload);
    if (!resolution
      || !samePoint(resolution.requested_point, proposedStart.point)
      || resolution.profile_version !== profileVersions[request.activity]) {
      this.update({ ...this.value, phase: "error", issue: "invalid_response" });
      return;
    }
    this.update({ ...this.value, phase: "awaiting_confirmation", resolution, issue: null });
  }

  private async confirmThenCreate(request: RouteRequest, proposedStart: ProposedStart, resolution: StartResolution, key: string, token: number): Promise<void> {
    if (!this.fenceAsyncStep(token, request.revision)) return;
    const csrf = this.csrfToken();
    if (!csrf) {
      this.update({ ...this.value, phase: "error", issue: "csrf_unavailable" });
      return;
    }
    try {
      const confirmation = await this.fetcher(`/api/v1/starts/${resolution.id}/confirm`, {
        method: "POST",
        credentials: "include",
        headers: jsonHeaders(csrf),
        body: JSON.stringify({ profile: profile(request.activity) }),
      });
      const confirmationPayload = await responseJson(confirmation);
      if (!this.fenceAsyncStep(token, request.revision)) return;
      if (!confirmation.ok) {
        this.update({ ...this.value, phase: "error", issue: decodeStartOutcome(confirmationPayload) ?? "invalid_response" });
        return;
      }
      const confirmed = decodeConfirmedStart(confirmationPayload);
      if (!confirmed || !samePoint(confirmed, resolution.candidate_network_point)) {
        this.update({ ...this.value, phase: "error", issue: "invalid_response" });
        return;
      }
      const template = templateIdentities[request.design];
      if (!template) {
        this.update({ ...this.value, phase: "error", issue: "invalid_fields" });
        return;
      }
      const requestedMetres = Math.round(request.distanceKm * 1_000);
      const toleranceMetres = Math.round(requestedMetres * request.tolerancePercent / 100);
      this.pendingCreation = {
        request,
        proposedStart,
        resolution,
        key,
        body: JSON.stringify({
          resolved_start_id: resolution.id,
          revision: request.revision,
          idempotency_key: key,
          requested_distance_metres: requestedMetres,
          distance_tolerance_metres: toleranceMetres,
          return_to_start: request.returnsToStart,
          profile: profile(request.activity),
          coverage_policy_version: resolution.coverage_policy_version,
          template_id: template.id,
          template_version: template.version,
          map_data_version: resolution.data_version,
          algorithm_version: "algorithm-v1",
        }),
      };
      await this.createPending(token);
    } catch {
      if (this.fenceAsyncStep(token, request.revision)) {
        this.update({ ...this.value, phase: "error", issue: "network_unavailable" });
      }
    }
  }

  private retryPendingCreation(): Promise<void> {
    const pending = this.pendingCreation;
    if (!pending) return Promise.resolve();
    this.stopTimer();
    const token = ++this.generation;
    this.update({
      ...emptySnapshot(),
      phase: "submitting",
      request: pending.request,
      proposedStart: pending.proposedStart,
      resolution: pending.resolution,
    });
    const operation = this.createPending(token)
      .finally(() => {
        if (this.submitting === operation) this.submitting = null;
      });
    this.submitting = operation;
    return operation;
  }

  private async createPending(token: number): Promise<void> {
    const pending = this.pendingCreation;
    if (!pending) return;
    const csrf = this.csrfToken();
    if (!csrf) {
      if (token === this.generation) {
        this.update({ ...this.value, phase: "error", issue: "csrf_unavailable" });
      }
      return;
    }
    let creation: Response;
    try {
      creation = await this.fetcher("/api/v1/route-jobs", {
        method: "POST",
        credentials: "include",
        headers: jsonHeaders(csrf),
        body: pending.body,
      });
    } catch {
      if (token === this.generation) {
        this.update({ ...this.value, phase: "error", issue: "network_unavailable" });
      }
      return;
    }
    const creationPayload = await responseJson(creation);
    if (!creation.ok) {
      if (creation.status < 500) this.pendingCreation = null;
      if (token === this.generation) {
        this.update({ ...this.value, phase: "error", issue: decodeRouteJobError(creationPayload) ?? "invalid_response" });
      }
      return;
    }
    const accepted = decodeAcceptedRouteJob(creationPayload);
    if (!accepted || accepted.requestRevision !== pending.request.revision) {
      if (token === this.generation) {
        this.update({ ...this.value, phase: "error", issue: "invalid_response" });
      }
      return;
    }
    const acceptedForCurrentRequest = token === this.generation
      && pending.request.revision === this.currentRevision();
    const active: ActiveJob = {
      version: 1,
      jobId: accepted.jobId,
      request: pending.request,
      proposedStart: pending.proposedStart,
      expiresAtEpochSeconds: null,
      etag: null,
      statusPayload: null,
    };
    this.pendingCreation = null;
    this.active = active;
    this.submitFingerprint = JSON.stringify({ request: pending.request, proposedStart: pending.proposedStart });
    this.submissionKey = pending.key;
    storeActive(this.storage, active);
    const pollToken = ++this.generation;
    this.update({
      ...this.value,
      phase: "polling",
      request: pending.request,
      proposedStart: pending.proposedStart,
      resolution: pending.resolution,
      jobId: accepted.jobId,
      status: null,
      issue: acceptedForCurrentRequest ? null : "active_job",
    });
    this.schedulePoll(pollToken, MIN_POLL_MILLISECONDS);
  }

  private async poll(token: number): Promise<void> {
    const active = this.active;
    if (!active || token !== this.generation) return;
    if (active.expiresAtEpochSeconds !== null
      && active.expiresAtEpochSeconds <= Math.floor(this.now() / 1_000)) {
      this.removeActive();
      this.update({ ...this.value, phase: "error", jobId: null, issue: "expired_job" });
      return;
    }
    const headers: Record<string, string> = {};
    if (active.etag && active.statusPayload !== null) headers["If-None-Match"] = active.etag;
    let response: Response;
    try {
      response = await this.fetcher(`/api/v1/route-jobs/${active.jobId}`, {
        credentials: "include",
        headers,
      });
    } catch {
      if (token !== this.generation) return;
      this.pollDelay = Math.min(MAX_POLL_MILLISECONDS, this.pollDelay * 2);
      this.schedulePoll(token, this.pollDelay);
      return;
    }
    if (token !== this.generation || this.active?.jobId !== active.jobId) return;
    if (response.status === 304) {
      this.pollDelay = Math.min(MAX_POLL_MILLISECONDS, this.pollDelay * 2);
      this.schedulePoll(token, this.pollDelay);
      return;
    }
    const payload = await responseJson(response);
    if (!response.ok) {
      if (response.status >= 500) {
        this.pollDelay = Math.min(MAX_POLL_MILLISECONDS, this.pollDelay * 2);
        this.schedulePoll(token, this.pollDelay);
      } else {
        this.update({ ...this.value, phase: "error", issue: decodeRouteJobError(payload) ?? "invalid_response" });
      }
      return;
    }
    const status = decodeRouteJobStatus(payload);
    if (!status || status.jobId !== active.jobId || status.requestRevision !== active.request.revision) {
      this.update({ ...this.value, phase: "error", issue: "invalid_response" });
      return;
    }
    const etag = response.headers.get("etag");
    active.etag = etag;
    active.statusPayload = payload;
    active.expiresAtEpochSeconds = status.expiresAtEpochSeconds;
    storeActive(this.storage, active);
    if (status.state === "queued" || status.state === "running") {
      this.pollDelay = Math.max(MIN_POLL_MILLISECONDS, Math.min(MAX_POLL_MILLISECONDS, (status.pollAfterSeconds ?? 1) * 1_000));
      this.update({ ...this.value, phase: "polling", status, issue: null });
      this.schedulePoll(token, this.pollDelay);
      return;
    }
    this.removeActive();
    this.resetLogicalSubmission();
    if (status.requestRevision === this.currentRevision()) {
      this.update({ ...this.value, phase: "terminal", jobId: null, status, issue: null });
    } else {
      this.update({ ...this.value, phase: "stale", jobId: null, status: null, previousStatus: status, issue: null });
    }
  }

  private schedulePoll(token: number, delay: number): void {
    this.stopTimer();
    this.timer = this.schedule(() => { void this.poll(token); }, delay);
  }

  private stopTimer(): void {
    if (this.timer !== undefined) this.cancelSchedule(this.timer);
    this.timer = undefined;
  }

  private removeActive(): void {
    this.stopTimer();
    this.active = null;
    try { this.storage.removeItem(ACTIVE_JOB_STORAGE_KEY); } catch { /* storage is optional */ }
  }

  private update(next: PlannerWorkflowSnapshot): void {
    this.value = Object.freeze(next);
    this.onChange(this.value);
  }

  private resetLogicalSubmission(): void {
    this.submitFingerprint = null;
    this.submissionKey = null;
    this.pendingCreation = null;
  }

  private fenceAsyncStep(token: number, revision: number): boolean {
    if (token !== this.generation) return false;
    if (revision !== this.currentRevision()) {
      this.update({ ...this.value, phase: "stale", resolution: null, issue: null });
      return false;
    }
    return true;
  }
}

function emptySnapshot(): PlannerWorkflowSnapshot {
  return Object.freeze({
    phase: "idle", request: null, proposedStart: null, resolution: null,
    jobId: null, status: null, previousStatus: null, issue: null,
  });
}

function profile(activity: RouteRequest["activity"]): { activity: string; profile_version: ProfileVersion } {
  return { activity: activity.toLowerCase(), profile_version: profileVersions[activity] };
}

function validRequest(request: RouteRequest): boolean {
  return typeof request.start === "string" && request.start.length > 0 && request.start.length <= 500
    && ["Walking", "Running", "Cycling"].includes(request.activity)
    && Number.isSafeInteger(request.revision) && request.revision > 0
    && Number.isFinite(request.distanceKm) && request.distanceKm >= 1 && request.distanceKm <= 100
    && Number.isFinite(request.tolerancePercent) && request.tolerancePercent >= 0 && request.tolerancePercent <= 100
    && typeof request.returnsToStart === "boolean"
    && templateIdentities[request.design] !== undefined;
}

function validProposedStart(start: ProposedStart): boolean {
  const accuracyIsValid = start.source === "geolocation"
    ? Number.isFinite(start.accuracyMetres) && Number(start.accuracyMetres) >= 0 && Number(start.accuracyMetres) <= 100
    : start.accuracyMetres === undefined;
  return ["manual", "search", "map", "geolocation"].includes(start.source)
    && typeof start.label === "string" && start.label.length > 0 && start.label.length <= 500
    && !!start.point && Number.isFinite(start.point.latitude) && Number.isFinite(start.point.longitude)
    && start.point.latitude >= -90 && start.point.latitude <= 90
    && start.point.longitude >= -180 && start.point.longitude <= 180
    && accuracyIsValid;
}

function samePoint(left: { latitude: number; longitude: number }, right: { latitude: number; longitude: number }): boolean {
  return left.latitude === right.latitude && left.longitude === right.longitude;
}

function jsonHeaders(csrf: string): Record<string, string> {
  return { "Content-Type": "application/json", "X-CSRF-Token": csrf };
}

async function responseJson(response: Response): Promise<unknown> {
  try { return await response.json(); } catch { return undefined; }
}

function decodeCancellation(value: unknown): "succeeded" | "no_match" | "failed" | "cancelled" | "timed_out" | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const raw = value as Record<string, unknown>;
  if (Object.keys(raw).length !== 1 || typeof raw.state !== "string") return null;
  return ["succeeded", "no_match", "failed", "cancelled", "timed_out"].includes(raw.state)
    ? raw.state as "succeeded" | "no_match" | "failed" | "cancelled" | "timed_out"
    : null;
}

function loadActive(storage: StoragePort): ActiveJob | null {
  try {
    const raw = storage.getItem(ACTIVE_JOB_STORAGE_KEY);
    if (!raw) return null;
    const value = JSON.parse(raw) as Record<string, unknown>;
    if (value.version !== 1 || typeof value.jobId !== "string" || !isUuidV4(value.jobId)
      || !value.request || typeof value.request !== "object"
      || !value.proposedStart || typeof value.proposedStart !== "object"
      || (value.expiresAtEpochSeconds !== null
        && (!Number.isSafeInteger(value.expiresAtEpochSeconds) || Number(value.expiresAtEpochSeconds) <= 0))
      || (value.etag !== null && typeof value.etag !== "string")) return null;
    const request = value.request as RouteRequest;
    const proposedStart = value.proposedStart as ProposedStart;
    if (!validRequest(request) || !validProposedStart(proposedStart) || request.start !== proposedStart.label) return null;
    const statusPayload = value.statusPayload ?? null;
    const status = statusPayload === null ? null : decodeRouteJobStatus(statusPayload);
    if (status && (status.jobId !== value.jobId || status.requestRevision !== request.revision
      || status.state !== "queued" && status.state !== "running"
      || value.expiresAtEpochSeconds !== status.expiresAtEpochSeconds)) return null;
    return {
      version: 1,
      jobId: value.jobId,
      request,
      proposedStart,
      expiresAtEpochSeconds: value.expiresAtEpochSeconds === null ? null : Number(value.expiresAtEpochSeconds),
      etag: status ? value.etag as string | null : null,
      statusPayload: status ? statusPayload : null,
    };
  } catch {
    return null;
  }
}

function storeActive(storage: StoragePort, active: ActiveJob): void {
  try { storage.setItem(ACTIVE_JOB_STORAGE_KEY, JSON.stringify(active)); } catch { /* polling remains available in memory */ }
}

function isUuidV4(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}
