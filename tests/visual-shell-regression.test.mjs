import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("the visual shell is bootstrapped by native TypeScript only", async () => {
  const bootstrap = await readFile(new URL("../src/main.ts", import.meta.url), "utf8");
  assert.match(bootstrap, /import "\.\/app\.ts"/);
  assert.doesNotMatch(bootstrap, /app\.js/);
  await assert.rejects(readFile(new URL("../src/app.js", import.meta.url), "utf8"));
});

test("the active TypeScript controller cannot fabricate route candidates or timed success", async () => {
  const controller = await readFile(new URL("../src/app.ts", import.meta.url), "utf8");
  assert.doesNotMatch(controller, /shapeGeometry|baseHeart|exportGpx|setTimeout\([^)]*results/);
  assert.match(controller, /loadRouteSnapshots/);
});
