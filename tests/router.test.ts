import assert from "node:assert/strict";
import test from "node:test";
import { currentScreen, screens } from "../src/app/router.ts";

test("every review screen has a reachable hash route", () => {
  for (const screen of screens) assert.equal(currentScreen(`/#/${screen}`.replace("/#", "#"), undefined), screen);
  assert.equal(currentScreen("#/unknown", undefined), "planner");
});
