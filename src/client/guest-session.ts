import {
  decodeGuestSessionPolicy,
  scheduleGuestSessionRotation,
  type GuestSessionPolicy,
} from "../domain/guest-session.ts";

export type RotationScheduler = (
  policy: GuestSessionPolicy,
  rotate: () => Promise<void>,
) => () => void;

export async function startGuestSessionMaintenance(
  fetcher: typeof fetch = globalThis.fetch,
  csrfToken: () => string | null = readCsrfCookie,
  scheduler: RotationScheduler = (policy, rotate) =>
    scheduleGuestSessionRotation(policy, rotate),
): Promise<() => void> {
  const issued = await fetcher("/api/v1/guest-sessions", {
    method: "POST",
    credentials: "include",
  });
  if (!issued.ok) throw new Error("guest_session_unavailable");
  const policy = decodeGuestSessionPolicy(await issued.json());
  if (!policy) throw new Error("invalid_guest_session_policy");

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
      schedule(replacement);
    });
  };
  schedule(policy);
  return () => cancel();
}

function readCsrfCookie(): string | null {
  const pair = globalThis.document?.cookie
    .split(";")
    .map((value) => value.trim())
    .find((value) => value.startsWith("gpsart_csrf="));
  return pair ? decodeURIComponent(pair.slice("gpsart_csrf=".length)) : null;
}
