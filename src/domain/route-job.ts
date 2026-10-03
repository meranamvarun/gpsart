export interface AcceptedRouteJob { readonly jobId: string; readonly state: "queued"; readonly requestRevision: number; }

export function decodeAcceptedRouteJob(value: unknown): AcceptedRouteJob | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const raw = value as Record<string, unknown>;
  if (typeof raw.job_id !== "string" || !/^[0-9a-f-]{36}$/i.test(raw.job_id) || raw.state !== "queued" || !Number.isSafeInteger(raw.request_revision) || Number(raw.request_revision) < 1) return null;
  return Object.freeze({ jobId: raw.job_id, state: "queued", requestRevision: Number(raw.request_revision) });
}

export type RouteJobErrorCode = "invalid_request" | "not_found" | "idempotency_conflict" | "job_state_conflict" | "job_storage_unavailable";
export function decodeRouteJobError(value: unknown): RouteJobErrorCode | null {
  if (typeof value !== "object" || value === null) return null;
  const code = (value as Record<string, unknown>).code;
  return ["invalid_request", "not_found", "idempotency_conflict", "job_state_conflict", "job_storage_unavailable"].includes(String(code)) ? code as RouteJobErrorCode : null;
}
