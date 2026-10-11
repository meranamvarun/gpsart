import assert from "node:assert/strict";
import test from "node:test";

import { baseMapTileErrorHandler, renderMap } from "../src/app/map.ts";
import { renderScreen } from "../src/app/screen-controller.ts";
import { acceptCandidateLoad, beginCandidateLoad, initialState } from "../src/app/state.ts";

const savedRoutes = { kind: "available" as const, routes: [], errors: [] };

test("production empty, loading, no-match, dependency-error, and map-unavailable states need no route fixture", () => {
  const empty = initialState();
  assert.match(renderScreen("results", empty, savedRoutes), /No route geometry is displayed/i);

  const loading = beginCandidateLoad(empty, empty.request.revision);
  assert.match(renderScreen("results", loading, savedRoutes), /Loading complete validated route snapshots/i);

  const unavailable = acceptCandidateLoad(loading, empty.request.revision, { kind: "unavailable" });
  assert.match(renderScreen("results", unavailable, savedRoutes), /snapshots could not be loaded/i);
  assert.match(renderScreen("no-match", empty, savedRoutes), /No qualifying result was returned/i);
  assert.match(renderScreen("error", empty, savedRoutes), /could not complete this request/i);
  assert.match(renderMap("results", empty), /Route geometry unavailable/i);

  const mapStatus = { innerHTML: "Complete routed geometry remains visible" };
  baseMapTileErrorHandler(mapStatus)();
  assert.match(mapStatus.innerHTML, /Base map unavailable/i);
});

test("design-review index is visibly labelled and never claims backend success", () => {
  const markup = renderScreen("screens", initialState(), savedRoutes);
  assert.match(markup, /GPSART.*Design review/i);
  assert.match(markup, /without simulating backend success/i);
});
