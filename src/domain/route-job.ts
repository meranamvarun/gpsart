export interface AcceptedRouteJob { readonly jobId: string; readonly requestRevision: number; }

export function decodeAcceptedRouteJob(value: unknown): AcceptedRouteJob | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const raw = value as Record<string, unknown>;
  if (Object.keys(raw).some((key) => key !== "job_id" && key !== "request_revision")) return null;
  if (typeof raw.job_id !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(raw.job_id) || !Number.isSafeInteger(raw.request_revision) || Number(raw.request_revision) < 1) return null;
  return Object.freeze({ jobId: raw.job_id, requestRevision: Number(raw.request_revision) });
}

export type RouteJobErrorCode = "invalid_request" | "not_found" | "idempotency_conflict" | "job_state_conflict" | "job_storage_unavailable";
export function decodeRouteJobError(value: unknown): RouteJobErrorCode | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const raw = value as Record<string, unknown>;
  if (Object.keys(raw).some((key) => key !== "code")) return null;
  const code = raw.code;
  return typeof code === "string" && ["invalid_request", "not_found", "idempotency_conflict", "job_state_conflict", "job_storage_unavailable"].includes(code) ? code as RouteJobErrorCode : null;
}
