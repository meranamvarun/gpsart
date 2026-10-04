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

export type CandidateSnapshotLoad =
  | { readonly kind: "available"; readonly snapshots: readonly RouteSnapshot[] }
  | { readonly kind: "unavailable" }
  | { readonly kind: "invalid_response" };

export async function fetchRouteCandidates(
  resultIds: readonly string[],
  fetcher: typeof fetch = fetch,
): Promise<CandidateSnapshotLoad> {
  const uniqueIds = [...new Set(resultIds)];
  if (uniqueIds.length < 1 || uniqueIds.length > 3 || uniqueIds.length !== resultIds.length) return { kind: "invalid_response" };
  let responses: Response[];
  try {
    responses = await Promise.all(uniqueIds.map((id) => fetcher(`/api/v1/routes/${encodeURIComponent(id)}`, {
      method: "GET",
      credentials: "same-origin",
      cache: "no-store",
      headers: { Accept: "application/json" },
    })));
  } catch {
    return { kind: "unavailable" };
  }
  if (responses.some((response) => !response.ok)) return { kind: "unavailable" };
  let payloads: unknown[];
  try {
    payloads = await Promise.all(responses.map((response) => response.json() as Promise<unknown>));
  } catch {
    return { kind: "invalid_response" };
  }
  const snapshots = payloads.map(decodeRouteSnapshot);
  if (snapshots.some((snapshot) => snapshot === null)) return { kind: "invalid_response" };
  const decoded = snapshots as RouteSnapshot[];
  if (decoded.some((snapshot, index) => snapshot.id !== uniqueIds[index])) return { kind: "invalid_response" };
  return { kind: "available", snapshots: Object.freeze(decoded) };
}
