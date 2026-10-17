import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { renderScreen } from "../src/app/screen-controller.ts";
import { acceptCatalogueLoad, initialState } from "../src/app/state.ts";

const savedRoutes = { kind: "available" as const, routes: [], errors: [] };
const published = [
  { id: "heart", version: 1, status: "published" as const, category: "shape" as const, name: "Heart", accessibleName: "Heart route design", complexityHint: "simple", previewSvg: "<svg></svg>", definitionHash: "a".repeat(64) },
  { id: "HI", version: 1, status: "published" as const, category: "text" as const, name: "HI", accessibleName: "HI text route design", complexityHint: "moderate", previewSvg: "<svg></svg>", definitionHash: "b".repeat(64) },
];

test("planner identifies the bounded Bengaluru pilot and calibrated first-release designs", () => {
  const state = acceptCatalogueLoad(initialState(), { kind: "available", designs: published });
  const markup = renderScreen("planner", state, savedRoutes);

  assert.match(markup, /Bengaluru pilot/i);
  assert.match(markup, /only Heart and HI (?:are )?calibrated/i);
  assert.match(markup, /coverage/i);
  assert.doesNotMatch(markup, /explore your city/i);
  assert.doesNotMatch(markup, /place-name|search for a place/i);
});

test("catalogue states that arbitrary starts and custom text are not supported", () => {
  const state = acceptCatalogueLoad(initialState(), { kind: "available", designs: published });
  const markup = renderScreen("catalogue", state, savedRoutes);

  assert.match(markup, /Bengaluru pilot/i);
  assert.match(markup, /only Heart and HI (?:are )?calibrated/i);
  assert.match(markup, /not promised at every start/i);
  assert.match(markup, /custom words? (?:are|is) unavailable/i);
});

test("production controller has no authentication or place-search activation bridge", async () => {
  const controller = await readFile(new URL("../src/app.ts", import.meta.url), "utf8");
  assert.doesNotMatch(controller, /signin|data-action === "google"|gpsart:search-selection|fromSearchSelection/i);
});

test("guest planner and device-only saved routes remain public without account language", () => {
  const planner = renderScreen("planner", initialState(), savedRoutes);
  const saved = renderScreen("saved", initialState(), savedRoutes);

  assert.match(planner, /id="plan-form"/);
  assert.match(saved, /Saved on this device/i);
  assert.match(saved, /Clearing this site data removes/i);
  assert.doesNotMatch(`${planner}${saved}`, /sign(?:ed|ing)? in|account required/i);
});
