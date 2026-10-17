import assert from "node:assert/strict";
import test from "node:test";
import { currentScreen, screens } from "../src/app/router.ts";

test("every supported screen has a reachable hash route", () => {
  for (const screen of screens) assert.equal(currentScreen(`/#/${screen}`.replace("/#", "#"), undefined, true), screen);
  assert.equal(currentScreen("#/unknown", undefined), "planner");
});

test("only the review index requires build-time review mode", () => {
  assert.equal(currentScreen("#/screens", undefined), "planner");
  assert.equal(currentScreen("#/screens", undefined, true), "screens");
  assert.equal(currentScreen("#/screens?designReview=true", undefined), "planner");
});

test("stale authentication URLs and query or review flags cannot expose unsupported account actions", () => {
  for (const route of ["login", "auth-pending", "auth-error", "welcome", "account"]) {
    assert.equal(currentScreen(`#/${route}`, route, false), "planner");
    assert.equal(currentScreen(`#/${route}?enabled=true`, route, true), "planner");
  }
});
