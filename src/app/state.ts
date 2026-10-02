import type { ApiRouteResult } from "../client/api.ts";
import { isCurrentResult, type RouteRequest, reviseRequest } from "../domain/planner.ts";
import type { RouteSnapshot } from "../domain/route-snapshot.ts";
import type { DesignCategory } from "./catalogue.ts";

export type RouteOutcome = "idle" | "pending" | "route" | "no_match" | "unavailable" | "invalid_response" | "stale";

export interface AppState {
  readonly request: RouteRequest;
  readonly signedIn: boolean;
  readonly category: DesignCategory;
  readonly mapExpanded: boolean;
  readonly outcome: RouteOutcome;
  readonly routeResult: RouteSnapshot | null;
}

export function initialState(): AppState {
  return {
    request: { start: "Koramangala, Bengaluru", activity: "Cycling", design: "Heart", distanceKm: 15, tolerancePercent: 10, returnsToStart: true, revision: 0 },
    signedIn: false,
    category: "Shapes",
    mapExpanded: false,
    outcome: "idle",
    routeResult: null,
  };
}

export function updateRequest(state: AppState, changes: Omit<Partial<RouteRequest>, "revision">): AppState {
  return { ...state, request: reviseRequest(state.request, changes), outcome: "idle", routeResult: null };
}

export function beginRequest(state: AppState): AppState {
  return { ...state, outcome: "pending", routeResult: null };
}

export function acceptApiResult(state: AppState, result: ApiRouteResult): AppState {
  if (result.kind !== "route") return { ...state, outcome: result.kind, routeResult: null };
  if (!isCurrentResult(state.request, result.snapshot.requestRevision)) return { ...state, outcome: "stale", routeResult: null };
  return { ...state, outcome: "route", routeResult: result.snapshot };
}

export function updateUi(state: AppState, changes: Partial<Pick<AppState, "signedIn" | "category" | "mapExpanded">>): AppState {
  return { ...state, ...changes };
}
