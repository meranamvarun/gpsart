import type { ApiRouteResult } from "../client/api.ts";
import type { CandidateSnapshotLoad } from "../client/api.ts";
import { isCurrentResult, type RouteRequest, reviseRequest } from "../domain/planner.ts";
import type { RouteSnapshot } from "../domain/route-snapshot.ts";
import type { RouteJobStatus } from "../domain/job-status.ts";
import type { DesignCategory } from "./catalogue.ts";
import type { DesignCatalogueLoad, DesignCatalogueState } from "../domain/design-template.ts";

export type RouteOutcome = "idle" | "pending" | "route" | "no_match" | "unavailable" | "invalid_response" | "stale";

export type CompletedAttempt = Readonly<{
  request: RouteRequest;
  status: RouteJobStatus;
}>;

export interface AppState {
  readonly request: RouteRequest;
  readonly signedIn: boolean;
  readonly category: DesignCategory;
  readonly catalogue: DesignCatalogueState;
  readonly mapExpanded: boolean;
  readonly outcome: RouteOutcome;
  readonly routeResult: RouteSnapshot | null;
  readonly routeResults: readonly RouteSnapshot[];
  readonly candidateLoad: "idle" | "loading" | "available" | "unavailable" | "invalid_response";
  readonly candidateRequestRevision: number | null;
  readonly selectedCandidateId: string | null;
  readonly lastAttempt: CompletedAttempt | null;
}

export function initialState(): AppState {
  return {
    request: { start: "", activity: "Cycling", design: "Heart", distanceKm: 15, tolerancePercent: 10, returnsToStart: true, revision: 0 },
    signedIn: false,
    category: "Shapes",
    catalogue: Object.freeze({ kind: "loading" }),
    mapExpanded: false,
    outcome: "idle",
    routeResult: null,
    routeResults: Object.freeze([]),
    candidateLoad: "idle",
    candidateRequestRevision: null,
    selectedCandidateId: null,
    lastAttempt: null,
  };
}

export function updateRequest(state: AppState, changes: Omit<Partial<RouteRequest>, "revision">): AppState {
  return { ...state, request: reviseRequest(state.request, changes), outcome: "idle", routeResult: null, routeResults: Object.freeze([]), candidateLoad: "idle", candidateRequestRevision: null, selectedCandidateId: null };
}

export function beginRequest(state: AppState): AppState {
  return { ...state, outcome: "pending", routeResult: null };
}

export function acceptApiResult(state: AppState, result: ApiRouteResult): AppState {
  if (result.kind !== "route") return { ...state, outcome: result.kind, routeResult: null };
  if (!isCurrentResult(state.request, result.snapshot.requestRevision)) return { ...state, outcome: "stale", routeResult: null };
  return { ...state, outcome: "route", routeResult: result.snapshot };
}

export function acceptCatalogueLoad(state: AppState, result: DesignCatalogueLoad): AppState {
  return { ...state, catalogue: result };
}

export function beginCandidateLoad(state: AppState, requestRevision: number): AppState {
  if (state.candidateRequestRevision !== null && requestRevision < state.candidateRequestRevision) return state;
  return { ...state, routeResult: null, routeResults: Object.freeze([]), candidateLoad: "loading", candidateRequestRevision: requestRevision, selectedCandidateId: null };
}

export function acceptCandidateLoad(state: AppState, requestRevision: number, result: CandidateSnapshotLoad): AppState {
  if (state.candidateRequestRevision !== requestRevision || state.candidateLoad !== "loading") return state;
  if (result.kind !== "available") return { ...state, candidateLoad: result.kind, routeResult: null, routeResults: Object.freeze([]), selectedCandidateId: null };
  if (result.snapshots.some((snapshot) => snapshot.requestRevision !== requestRevision)) {
    return { ...state, candidateLoad: "invalid_response", routeResult: null, routeResults: Object.freeze([]), selectedCandidateId: null };
  }
  const snapshots = Object.freeze([...result.snapshots]);
  return { ...state, candidateLoad: "available", routeResults: snapshots, routeResult: snapshots[0] ?? null, selectedCandidateId: snapshots[0]?.id ?? null };
}

export function selectCandidate(state: AppState, candidateId: string): AppState {
  const selected = state.routeResults.find(({ id }) => id === candidateId);
  return selected ? { ...state, selectedCandidateId: selected.id, routeResult: selected } : state;
}

export function recordCompletedAttempt(state: AppState, request: RouteRequest, status: RouteJobStatus): AppState {
  if (status.requestRevision !== request.revision || status.state === "queued" || status.state === "running") return state;
  if (state.lastAttempt && state.lastAttempt.status.requestRevision > status.requestRevision) return state;
  const retainedStatus: RouteJobStatus = Object.freeze({
    ...status,
    outcome: status.outcome ? Object.freeze({ ...status.outcome }) : null,
    resultIds: Object.freeze([...status.resultIds]),
  });
  return {
    ...state,
    lastAttempt: Object.freeze({
      request: Object.freeze({ ...request }),
      status: retainedStatus,
    }),
  };
}

export function retryCompletedAttempt(state: AppState): AppState | null {
  const attempt = state.lastAttempt;
  if (!attempt || !canRetryCompletedAttempt(state)) return null;
  return {
    ...state,
    request: reviseRequest(attempt.request, {}),
    outcome: "pending",
    routeResult: null,
  };
}

export function canRetryCompletedAttempt(state: AppState): boolean {
  const attempt = state.lastAttempt;
  return !!attempt?.status.outcome?.retryable && sameRequest(state.request, attempt.request);
}

export function updateUi(state: AppState, changes: Partial<Pick<AppState, "signedIn" | "category" | "mapExpanded">>): AppState {
  return { ...state, ...changes };
}

function sameRequest(left: RouteRequest, right: RouteRequest): boolean {
  return left.start === right.start
    && left.activity === right.activity
    && left.design === right.design
    && left.distanceKm === right.distanceKm
    && left.tolerancePercent === right.tolerancePercent
    && left.returnsToStart === right.returnsToStart
    && left.revision === right.revision;
}
