export interface GuestSessionPolicy {
  readonly expiresAt: number;
  readonly rotateAt: number;
  readonly rotateAfterSeconds: number;
}

export function decodeGuestSessionPolicy(value: unknown): GuestSessionPolicy | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const payload = value as Record<string, unknown>;
  if (!Number.isSafeInteger(payload.expires_at) || Number(payload.expires_at) <= 0) return null;
  if (!Number.isSafeInteger(payload.rotate_at) || Number(payload.rotate_at) <= 0) return null;
  if (!Number.isSafeInteger(payload.rotate_after) || Number(payload.rotate_after) <= 0) return null;
  if (Number(payload.rotate_at) >= Number(payload.expires_at)) return null;
  return Object.freeze({ expiresAt: Number(payload.expires_at), rotateAt: Number(payload.rotate_at), rotateAfterSeconds: Number(payload.rotate_after) });
}

type Schedule = (callback: () => void, delayMilliseconds: number) => unknown;
type Cancel = (handle: unknown) => void;

export function scheduleGuestSessionRotation(
  policy: GuestSessionPolicy,
  rotate: () => Promise<unknown>,
  now: () => number = Date.now,
  schedule: Schedule = (callback, delay) => globalThis.setTimeout(callback, delay),
  cancel: Cancel = (handle) => globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>),
): () => void {
  const delay = Math.max(0, policy.rotateAt * 1_000 - now());
  const handle = schedule(() => { void rotate(); }, delay);
  return () => cancel(handle);
}
