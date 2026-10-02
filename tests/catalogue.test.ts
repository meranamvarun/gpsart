import assert from "node:assert/strict";
import test from "node:test";
import { previewsFor, shapePreviews, textPreviews } from "../src/app/catalogue.ts";

test("the typed catalogue retains all currently reviewable designs", () => {
  assert.deepEqual(shapePreviews.map(({ name }) => name), ["Heart", "Circle", "Triangle", "Star", "Square"]);
  assert.deepEqual(textPreviews.map(({ name }) => name), ["HI", "GO", "RUN"]);
  assert.equal(previewsFor("Shapes"), shapePreviews);
  assert.equal(previewsFor("Text"), textPreviews);
});
