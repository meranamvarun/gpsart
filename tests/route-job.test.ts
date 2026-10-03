import assert from "node:assert/strict";
import test from "node:test";
import { decodeAcceptedRouteJob, decodeRouteJobError } from "../src/domain/route-job.ts";

test("route-job creation and errors decode without guessing", () => {
  assert.deepEqual(decodeAcceptedRouteJob({ job_id: "7bf80ccf-68e8-49ec-a3e8-cad667bb95a0", state: "queued", request_revision: 2 }), { jobId: "7bf80ccf-68e8-49ec-a3e8-cad667bb95a0", state: "queued", requestRevision: 2 });
  assert.equal(decodeAcceptedRouteJob({ job_id: "x", state: "done", request_revision: 0 }), null);
  assert.equal(decodeRouteJobError({ code: "idempotency_conflict" }), "idempotency_conflict");
  assert.equal(decodeRouteJobError({ code: "mystery" }), null);
});
