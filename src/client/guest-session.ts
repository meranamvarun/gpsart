import {
  decodeGuestSessionPolicy,
  scheduleGuestSessionRotation,
  type GuestSessionPolicy,
} from "../domain/guest-session.ts";

export type RotationScheduler = (
  policy: GuestSessionPolicy,
  rotate: () => Promise<void>,
) => () => void;

export interface SessionPolicyStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

const POLICY_STORAGE_KEY = "gpsart.guest-policy.v1";

export async function startGuestSessionMaintenance(
  fetcher: typeof fetch = globalThis.fetch,
  csrfToken: () => string | null = readCsrfCookie,
  scheduler: RotationScheduler = (policy, rotate) =>
    scheduleGuestSessionRotation(policy, rotate),
  policyStorage: SessionPolicyStorage = globalThis.localStorage,
  now: () => number = Date.now,
): Promise<() => void> {
  let policy = existingPolicy(policyStorage, csrfToken(), now());
  if (!policy) {
    const issued = await fetcher("/api/v1/guest-sessions", {
      method: "POST",
      credentials: "include",
    });
    if (!issued.ok) throw new Error("guest_session_unavailable");
    policy = decodeGuestSessionPolicy(await issued.json());
    if (!policy) throw new Error("invalid_guest_session_policy");
    storePolicy(policyStorage, policy);
  }

  let cancel: () => void = () => undefined;
  const schedule = (current: GuestSessionPolicy): void => {
    cancel = scheduler(current, async () => {
      const csrf = csrfToken();
      if (!csrf) throw new Error("csrf_cookie_unavailable");
      const response = await fetcher("/api/v1/guest-sessions/current/rotate", {
        method: "POST",
        credentials: "include",
        headers: { "X-CSRF-Token": csrf },
      });
      if (!response.ok) throw new Error("guest_session_rotation_failed");
      const replacement = decodeGuestSessionPolicy(await response.json());
      if (!replacement) throw new Error("invalid_guest_session_policy");
      storePolicy(policyStorage, replacement);
      schedule(replacement);
    });
  };
  schedule(policy);
  return () => cancel();
}

function existingPolicy(storage: SessionPolicyStorage, csrf: string | null, nowMilliseconds: number): GuestSessionPolicy | null {
  if (!csrf) return null;
  try {
    const raw = storage.getItem(POLICY_STORAGE_KEY);
    const policy = raw ? decodeGuestSessionPolicy(JSON.parse(raw)) : null;
    if (policy && policy.expiresAt * 1_000 > nowMilliseconds) return policy;
    storage.removeItem(POLICY_STORAGE_KEY);
  } catch {
    // Storage is optional; the secure cookies remain the authorization boundary.
  }
  return null;
}

function storePolicy(storage: SessionPolicyStorage, policy: GuestSessionPolicy): void {
  try {
    storage.setItem(POLICY_STORAGE_KEY, JSON.stringify({
      expires_at: policy.expiresAt,
      rotate_at: policy.rotateAt,
      rotate_after: policy.rotateAfterSeconds,
    }));
  } catch {
    // A blocked storage API must not expose or weaken the cookie credential.
  }
}

export function readCsrfCookie(): string | null {
  const pair = globalThis.document?.cookie
    .split(";")
    .map((value) => value.trim())
    .find((value) => value.startsWith("gpsart_csrf="));
  return pair ? decodeURIComponent(pair.slice("gpsart_csrf=".length)) : null;
}
