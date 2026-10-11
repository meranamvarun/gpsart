import { decodeRouteSnapshot, type RouteSnapshot } from "../domain/route-snapshot.ts";
import { decodeDesignCatalogue, type DesignCatalogueLoad, type DesignCategory } from "../domain/design-template.ts";

export async function fetchDesignCatalogue(
  category?: DesignCategory,
  fetcher: typeof fetch = fetch,
): Promise<DesignCatalogueLoad> {
  const suffix = category ? `?category=${encodeURIComponent(category)}` : "";
  let response: Response;
  try {
    response = await fetcher(`/api/v1/designs${suffix}`, {
      method: "GET",
      credentials: "same-origin",
      cache: "no-store",
      headers: { Accept: "application/json" },
    });
  } catch {
    return { kind: "unavailable" };
  }
  if (!response.ok) return { kind: "unavailable" };
  try {
    const designs = decodeDesignCatalogue(await response.json());
    if (category && designs.some((design) => design.category !== category)) return { kind: "invalid_response" };
    return { kind: "available", designs };
  } catch {
    return { kind: "invalid_response" };
  }
}

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

export type AuthorizedCandidateResults = Readonly<{
  jobId: string;
  requestRevision: number;
  resultIds: readonly string[];
}>;

export async function fetchRouteCandidates(
  authorized: AuthorizedCandidateResults,
  fetcher: typeof fetch = fetch,
): Promise<CandidateSnapshotLoad> {
  const { jobId, requestRevision, resultIds } = authorized;
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(jobId)
    || !Number.isSafeInteger(requestRevision) || requestRevision < 1) return { kind: "invalid_response" };
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
  if (decoded.some((snapshot, index) => snapshot.id !== uniqueIds[index]
    || snapshot.generationJobId !== jobId
    || snapshot.requestRevision !== requestRevision)) return { kind: "invalid_response" };
  return { kind: "available", snapshots: Object.freeze(decoded) };
}
