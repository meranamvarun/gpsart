export interface GuestSessionPolicy {
  readonly expiresAt: number;
  readonly rotateAfterSeconds: number;
}

export function decodeGuestSessionPolicy(value: unknown): GuestSessionPolicy | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const payload = value as Record<string, unknown>;
  if (!Number.isSafeInteger(payload.expires_at) || Number(payload.expires_at) <= 0) return null;
  if (!Number.isSafeInteger(payload.rotate_after) || Number(payload.rotate_after) <= 0) return null;
  return Object.freeze({ expiresAt: Number(payload.expires_at), rotateAfterSeconds: Number(payload.rotate_after) });
}
