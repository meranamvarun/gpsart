import assert from "node:assert/strict";
import test from "node:test";
import { decodeGuestSessionPolicy, scheduleGuestSessionRotation } from "../src/domain/guest-session.ts";

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
