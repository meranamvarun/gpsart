export type RouteJobState = "queued" | "running" | "succeeded" | "no_match" | "failed" | "cancelled" | "timed_out";
export type RouteJobStage = "queued" | "checking_roads" | "fitting_design" | "comparing_routes";
export type RouteJobOutcomeCode =
  | "succeeded"
  | "unsupported_coverage"
  | "no_routable_access"
  | "disconnected_network"
  | "insufficient_visual_match"
  | "outside_distance"
  | "exhausted_budget"
  | "provider_unavailable"
  | "attempts_exhausted"
  | "cancelled"
  | "timed_out"
  | "legacy_unknown";

export type RouteJobOutcome = Readonly<{
  code: RouteJobOutcomeCode;
  budgetExhausted: boolean;
  retryable: boolean;
  retryAfterSeconds: number | null;
}>;

export type RouteJobStatus = Readonly<{
  jobId: string;
  requestRevision: number;
  state: RouteJobState;
  stage: RouteJobStage;
  statusVersion: number;
  queuedAtEpochSeconds: number;
  startedAtEpochSeconds: number | null;
  completedAtEpochSeconds: number | null;
  deadlineAtEpochSeconds: number;
  expiresAtEpochSeconds: number;
  outcome: RouteJobOutcome | null;
  resultIds: readonly string[];
  pollAfterSeconds: number | null;
}>;

const states: readonly RouteJobState[] = ["queued", "running", "succeeded", "no_match", "failed", "cancelled", "timed_out"];
const stages: readonly RouteJobStage[] = ["queued", "checking_roads", "fitting_design", "comparing_routes"];
const outcomes: readonly RouteJobOutcomeCode[] = [
  "succeeded", "unsupported_coverage", "no_routable_access", "disconnected_network",
  "insufficient_visual_match", "outside_distance", "exhausted_budget", "provider_unavailable",
  "attempts_exhausted", "cancelled", "timed_out", "legacy_unknown",
];
const noMatchOutcomes: readonly RouteJobOutcomeCode[] = [
  "unsupported_coverage", "no_routable_access", "disconnected_network",
  "insufficient_visual_match", "outside_distance", "exhausted_budget", "legacy_unknown",
];
const failureOutcomes: readonly RouteJobOutcomeCode[] = ["provider_unavailable", "attempts_exhausted", "legacy_unknown"];
const uuidV4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function exactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  return Object.keys(value).length === expected.length && Object.keys(value).every((key) => expected.includes(key));
}

function positiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) > 0;
}

function nullablePositiveInteger(value: unknown): value is number | null {
  return value === null || positiveInteger(value);
}

function decodeOutcome(value: unknown): RouteJobOutcome | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const raw = value as Record<string, unknown>;
  if (!exactKeys(raw, ["code", "budget_exhausted", "retryable", "retry_after_seconds"])) return undefined;
  if (typeof raw.code !== "string" || !outcomes.includes(raw.code as RouteJobOutcomeCode)) return undefined;
  if (typeof raw.budget_exhausted !== "boolean" || typeof raw.retryable !== "boolean") return undefined;
  if (raw.retry_after_seconds !== null && !positiveInteger(raw.retry_after_seconds)) return undefined;
  const code = raw.code as RouteJobOutcomeCode;
  if (raw.retryable !== (code === "provider_unavailable")) return undefined;
  if ((code === "provider_unavailable") !== (raw.retry_after_seconds === 30)) return undefined;
  if (["exhausted_budget", "timed_out"].includes(code) && raw.budget_exhausted !== true) return undefined;
  if (code !== "succeeded" && !["exhausted_budget", "timed_out"].includes(code) && raw.budget_exhausted) return undefined;
  return Object.freeze({
    code,
    budgetExhausted: raw.budget_exhausted,
    retryable: raw.retryable,
    retryAfterSeconds: raw.retry_after_seconds as number | null,
  });
}

export function decodeRouteJobStatus(value: unknown): RouteJobStatus | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const raw = value as Record<string, unknown>;
  if (!exactKeys(raw, [
    "job_id", "request_revision", "state", "stage", "status_version",
    "queued_at_epoch_seconds", "started_at_epoch_seconds", "completed_at_epoch_seconds",
    "deadline_at_epoch_seconds", "expires_at_epoch_seconds", "outcome", "result_ids",
    "poll_after_seconds",
  ])) return undefined;
  if (typeof raw.job_id !== "string" || !uuidV4.test(raw.job_id)) return undefined;
  if (!positiveInteger(raw.request_revision) || !positiveInteger(raw.status_version)) return undefined;
  if (typeof raw.state !== "string" || !states.includes(raw.state as RouteJobState)) return undefined;
  if (typeof raw.stage !== "string" || !stages.includes(raw.stage as RouteJobStage)) return undefined;
  if (!positiveInteger(raw.queued_at_epoch_seconds) || !nullablePositiveInteger(raw.started_at_epoch_seconds) ||
      !nullablePositiveInteger(raw.completed_at_epoch_seconds) || !positiveInteger(raw.deadline_at_epoch_seconds) ||
      !positiveInteger(raw.expires_at_epoch_seconds)) return undefined;
  if (raw.deadline_at_epoch_seconds <= raw.queued_at_epoch_seconds || raw.expires_at_epoch_seconds <= raw.deadline_at_epoch_seconds) return undefined;
  if (raw.started_at_epoch_seconds !== null && raw.started_at_epoch_seconds < raw.queued_at_epoch_seconds) return undefined;
  if (raw.completed_at_epoch_seconds !== null && raw.completed_at_epoch_seconds < raw.queued_at_epoch_seconds) return undefined;
  if (!Array.isArray(raw.result_ids) || raw.result_ids.some((id) => typeof id !== "string" || !uuidV4.test(id))) return undefined;
  if (new Set(raw.result_ids).size !== raw.result_ids.length) return undefined;

  const state = raw.state as RouteJobState;
  const active = state === "queued" || state === "running";
  const outcome = raw.outcome === null ? null : decodeOutcome(raw.outcome);
  if (raw.outcome !== null && !outcome) return undefined;
  if (active) {
    if (raw.completed_at_epoch_seconds !== null || outcome !== null || raw.result_ids.length !== 0 || !positiveInteger(raw.poll_after_seconds)) return undefined;
    if (state === "queued" && (raw.stage !== "queued" || raw.started_at_epoch_seconds !== null)) return undefined;
    if (state === "running" && (raw.stage === "queued" || raw.started_at_epoch_seconds === null)) return undefined;
  } else {
    if (raw.completed_at_epoch_seconds === null || !outcome || raw.poll_after_seconds !== null) return undefined;
    if (state === "succeeded" && (outcome.code !== "succeeded" || raw.result_ids.length === 0)) return undefined;
    if (state !== "succeeded" && raw.result_ids.length !== 0) return undefined;
    if (state === "no_match" && !noMatchOutcomes.includes(outcome.code)) return undefined;
    if (state === "failed" && !failureOutcomes.includes(outcome.code)) return undefined;
    if (state === "cancelled" && outcome.code !== "cancelled") return undefined;
    if (state === "timed_out" && outcome.code !== "timed_out") return undefined;
  }

  return Object.freeze({
    jobId: raw.job_id,
    requestRevision: raw.request_revision,
    state,
    stage: raw.stage as RouteJobStage,
    statusVersion: raw.status_version,
    queuedAtEpochSeconds: raw.queued_at_epoch_seconds,
    startedAtEpochSeconds: raw.started_at_epoch_seconds,
    completedAtEpochSeconds: raw.completed_at_epoch_seconds,
    deadlineAtEpochSeconds: raw.deadline_at_epoch_seconds,
    expiresAtEpochSeconds: raw.expires_at_epoch_seconds,
    outcome,
    resultIds: Object.freeze([...raw.result_ids]) as readonly string[],
    pollAfterSeconds: raw.poll_after_seconds as number | null,
  });
}
