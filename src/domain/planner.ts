export const MIN_DISTANCE_KM = 1;
export const MAX_DISTANCE_KM = 100;

export interface RouteRequest {
  readonly start: string;
  readonly activity: "Walking" | "Running" | "Cycling";
  readonly design: string;
  readonly distanceKm: number;
  readonly tolerancePercent: number;
  readonly returnsToStart: boolean;
  readonly revision: number;
}

export interface DistanceBounds { readonly minimumKm: number; readonly maximumKm: number; }

export function distanceBounds(distanceKm: number, tolerancePercent: number): DistanceBounds {
  if (!Number.isFinite(distanceKm) || distanceKm < MIN_DISTANCE_KM || distanceKm > MAX_DISTANCE_KM) {
    throw new RangeError(`distance must be between ${MIN_DISTANCE_KM} and ${MAX_DISTANCE_KM} km`);
  }
  if (!Number.isFinite(tolerancePercent) || tolerancePercent < 0 || tolerancePercent > 100) {
    throw new RangeError("tolerance must be between 0 and 100 percent");
  }
  return { minimumKm: distanceKm * (1 - tolerancePercent / 100), maximumKm: distanceKm * (1 + tolerancePercent / 100) };
}

export function reviseRequest(request: RouteRequest, changes: Omit<Partial<RouteRequest>, "revision">): RouteRequest {
  return { ...request, ...changes, revision: request.revision + 1 };
}

export function isCurrentResult(request: RouteRequest, resultRevision: number): boolean {
  return request.revision === resultRevision;
}
