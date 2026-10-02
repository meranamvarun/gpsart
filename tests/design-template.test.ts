import assert from "node:assert/strict";
import test from "node:test";
import { decodeDesignCatalogue } from "../src/domain/design-template.ts";

const valid = {
  designs: [{
    id: "heart", version: 1, status: "published", category: "shape", name: "Heart",
    complexity_hint: "simple", preview_svg: "<svg></svg>", definition_hash: "a".repeat(64),
  }],
};

test("design decoder retains immutable version identity", () => {
  const designs = decodeDesignCatalogue(valid);
  assert.deepEqual(designs[0], { id: "heart", version: 1, status: "published", category: "shape", name: "Heart", complexityHint: "simple", previewSvg: "<svg></svg>", definitionHash: "a".repeat(64) });
  assert.ok(Object.isFrozen(designs));
  assert.ok(Object.isFrozen(designs[0]));
});

test("design decoder rejects drafts, bad hashes, and unknown categories", () => {
  for (const patch of [{ status: "draft" }, { definition_hash: "unknown" }, { category: "word" }]) {
    assert.throws(() => decodeDesignCatalogue({ designs: [{ ...valid.designs[0], ...patch }] }));
  }
});
