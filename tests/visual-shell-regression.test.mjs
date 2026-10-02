import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("the TypeScript bootstrap retains the current visual shell during migration", async () => {
  const bootstrap = await readFile(new URL("../src/main.ts", import.meta.url), "utf8");
  assert.match(bootstrap, /import "\.\/app\.js"/);
});
