import assert from "node:assert/strict";
import test from "node:test";
import { currentScreen, screens } from "../src/app/router.ts";

test("every review screen has a reachable hash route", () => {
  for (const screen of screens) assert.equal(currentScreen(`/#/${screen}`.replace("/#", "#"), undefined, true), screen);
  assert.equal(currentScreen("#/unknown", undefined), "planner");
});

test("prototype review and unconnected success screens require the build-time review mode", () => {
  assert.equal(currentScreen("#/screens", undefined), "planner");
  assert.equal(currentScreen("#/welcome", undefined), "planner");
  assert.equal(currentScreen("#/screens", undefined, true), "screens");
  assert.equal(currentScreen("#/welcome", undefined, true), "welcome");
  assert.equal(currentScreen("#/screens?designReview=true", undefined), "planner");
});
