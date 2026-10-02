import { type RouteRequest, reviseRequest } from "../domain/planner.ts";

export interface AppState { request: RouteRequest; signedIn: boolean; }

export function initialState(): AppState {
  return { request: { start: "", activity: "Walking", design: "Heart", distanceKm: 10, tolerancePercent: 10, returnsToStart: true, revision: 0 }, signedIn: false };
}

export function updateRequest(state: AppState, changes: Omit<Partial<RouteRequest>, "revision">): AppState {
  return { ...state, request: reviseRequest(state.request, changes) };
}
