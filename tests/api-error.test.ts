import assert from "node:assert/strict";
import test from "node:test";
import { decodeApiError } from "../src/domain/api-error.ts";

test("the client decodes the malformed UUID path contract without guessing", () => {
  assert.equal(decodeApiError({ code: "invalid_path_parameter" }), "invalid_path_parameter");
  assert.equal(decodeApiError({ code: "authentication_required" }), "authentication_required");
  assert.equal(decodeApiError({ code: "request_forbidden" }), "request_forbidden");
  assert.equal(decodeApiError({ code: "invalid_path_parameter", detail: "not-a-uuid" }), undefined);
  assert.equal(decodeApiError({ code: "invalid_request" }), undefined);
  assert.equal(decodeApiError("invalid_path_parameter"), undefined);
});
