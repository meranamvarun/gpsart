import { decodeRouteSnapshot, type RouteSnapshot } from "../domain/route-snapshot.ts";

export type ApiRouteResult =
  | { readonly kind: "route"; readonly snapshot: RouteSnapshot }
  | { readonly kind: "no_match" }
  | { readonly kind: "unavailable" }
  | { readonly kind: "invalid_response" };

export function decodeApiRouteResult(value: unknown): ApiRouteResult {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return { kind: "invalid_response" };
  const payload = value as Record<string, unknown>;
  if (payload.kind === "no_match") return { kind: "no_match" };
  if (payload.kind === "unavailable") return { kind: "unavailable" };
  if (payload.kind !== "route") return { kind: "invalid_response" };
  const snapshot = decodeRouteSnapshot(payload.snapshot);
  return snapshot ? { kind: "route", snapshot } : { kind: "invalid_response" };
}
