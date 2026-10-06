import assert from "node:assert/strict";
import test from "node:test";
import { previewsFor, shapePreviews, textPreviews } from "../src/app/catalogue.ts";
import { fetchDesignCatalogue } from "../src/client/api.ts";

const published = Object.freeze([
  Object.freeze({ id: "heart", version: 1, status: "published" as const, category: "shape" as const, name: "Heart", accessibleName: "Heart route design", complexityHint: "simple", previewSvg: "<svg></svg>", definitionHash: "a".repeat(64) }),
  Object.freeze({ id: "HI", version: 1, status: "published" as const, category: "text" as const, name: "HI", accessibleName: "HI text route design", complexityHint: "moderate", previewSvg: "<svg></svg>", definitionHash: "b".repeat(64) }),
]);

test("the typed catalogue retains all currently reviewable designs", () => {
  assert.deepEqual(shapePreviews.map(({ name }) => name), ["Heart", "Circle", "Triangle", "Star", "Square"]);
  assert.deepEqual(textPreviews.map(({ name }) => name), ["HI", "GO", "RUN"]);
});

test("only server-published fixture-backed designs are enabled", () => {
  const state = { kind: "available" as const, designs: published };
  const shapes = previewsFor("Shapes", state);
  assert.equal(shapes.find(({ name }) => name === "Heart")?.enabled, true);
  const circle = shapes.find(({ name }) => name === "Circle");
  assert.equal(circle?.enabled, false);
  assert.match(circle?.statusReason ?? "", /reviewed legal routing fixture/i);
  assert.equal(previewsFor("Text", state).find(({ name }) => name === "HI")?.enabled, true);
});

test("catalogue loading is category-filtered and rejects malformed responses", async () => {
  let requested = "";
  const loaded = await fetchDesignCatalogue("text", async (input) => {
    requested = String(input);
    return new Response(JSON.stringify({ designs: [{
      id: "HI", version: 1, status: "published", category: "text", name: "HI",
      accessible_name: "HI text route design", complexity_hint: "moderate", preview_svg: "<svg></svg>", definition_hash: "b".repeat(64),
    }] }), { status: 200, headers: { "Content-Type": "application/json" } });
  });
  assert.equal(requested, "/api/v1/designs?category=text");
  assert.equal(loaded.kind, "available");
  assert.deepEqual(await fetchDesignCatalogue("shape", async () => new Response(JSON.stringify({ designs: [{
    id: "HI", version: 1, status: "published", category: "text", name: "HI",
    accessible_name: "HI text route design", complexity_hint: "moderate", preview_svg: "<svg></svg>", definition_hash: "b".repeat(64),
  }] }), { status: 200 })), { kind: "invalid_response" });
  assert.deepEqual(await fetchDesignCatalogue(undefined, async () => new Response("{}", { status: 200 })), { kind: "invalid_response" });
  assert.deepEqual(await fetchDesignCatalogue(undefined, async () => { throw new Error("offline"); }), { kind: "unavailable" });
});
