import assert from "node:assert/strict";
import test from "node:test";
import { decodeGuestSessionPolicy, scheduleGuestSessionRotation } from "../src/domain/guest-session.ts";
import { startGuestSessionMaintenance } from "../src/client/guest-session.ts";

class PolicyStorage {
  value: string | null = null;
  getItem(): string | null { return this.value; }
  setItem(_key: string, value: string): void { this.value = value; }
  removeItem(): void { this.value = null; }
}

test("guest session response exposes timing policy but no credential", () => {
  assert.deepEqual(decodeGuestSessionPolicy({ expires_at: 2_000_000_000, rotate_at: 1_999_956_800, rotate_after: 43_200 }), { expiresAt: 2_000_000_000, rotateAt: 1_999_956_800, rotateAfterSeconds: 43_200 });
  assert.equal(decodeGuestSessionPolicy({ expires_at: 0, rotate_after: 43_200 }), null);
  assert.equal(decodeGuestSessionPolicy({ expires_at: 2_000_000_000, rotate_after: "soon" }), null);
});

test("guest session rotation is scheduled from the server deadline", async () => {
  let delay = -1;
  let callback: (() => void) | undefined;
  let rotations = 0;
  const cancel = scheduleGuestSessionRotation(
    { expiresAt: 2_000_000_000, rotateAt: 1_000, rotateAfterSeconds: 43_200 },
    async () => { rotations += 1; },
    () => 900_000,
    (fn, ms) => { callback = fn; delay = ms; return 7; },
    () => undefined,
  );
  assert.equal(delay, 100_000);
  callback?.();
  await Promise.resolve();
  assert.equal(rotations, 1);
  cancel();
});

test("browser maintenance issues a session and posts due rotation with CSRF", async () => {
  const calls: Array<{ url: string; init: RequestInit | undefined }> = [];
  let due: (() => void) | undefined;
  const fetcher: typeof fetch = async (url, init) => {
    calls.push({ url: String(url), init });
    return new Response(JSON.stringify({ expires_at: 2_000, rotate_at: 1_000, rotate_after: 500 }), {
      status: calls.length === 1 ? 201 : 200,
      headers: { "content-type": "application/json" },
    });
  };
  await startGuestSessionMaintenance(fetcher, () => "csrf-value", (_policy, rotate) => {
    due = () => { void rotate(); };
    return () => undefined;
  });
  assert.equal(calls[0]?.url, "/api/v1/guest-sessions");
  assert.equal(calls[0]?.init?.credentials, "include");
  due?.();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(calls[1]?.url, "/api/v1/guest-sessions/current/rotate");
  assert.equal(new Headers(calls[1]?.init?.headers).get("x-csrf-token"), "csrf-value");
});

test("refresh reuses an unexpired cookie policy instead of replacing job ownership", async () => {
  const storage = new PolicyStorage();
  storage.value = JSON.stringify({ expires_at: 2_000_000_000, rotate_at: 1_999_956_800, rotate_after: 43_200 });
  let requests = 0;
  let scheduled = false;
  await startGuestSessionMaintenance(
    async () => { requests += 1; throw new Error("must not issue a replacement session"); },
    () => "existing-csrf",
    () => { scheduled = true; return () => undefined; },
    storage,
    () => 1_900_000_000_000,
  );
  assert.equal(requests, 0);
  assert.equal(scheduled, true);
});
