import type { RouteRequest } from "../domain/planner.ts";

export function isPilotActivityAvailable(activity: RouteRequest["activity"]): boolean {
  return activity === "Walking" || activity === "Running";
}
