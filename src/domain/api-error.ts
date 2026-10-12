export type ApiErrorCode = "invalid_path_parameter";

export function decodeApiError(value: unknown): ApiErrorCode | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const payload = value as Record<string, unknown>;
  if (Object.keys(payload).length !== 1) return undefined;
  return payload.code === "invalid_path_parameter" ? payload.code : undefined;
}
