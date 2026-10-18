import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { loadFactory, readyStories, validateStories } from "../scripts/factory.mjs";

test("factory backlog is valid and includes every documented technical story", async () => {
  const factory = await loadFactory();
  assert.deepEqual(validateStories(factory), []);
  assert.equal(factory.stories.length, 38);
  assert.deepEqual(factory.stories.filter((story) => story.dependsOn.length === 0).map((story) => story.id), ["TS-001", "TS-003"]);
});

test("only stories with completed dependencies are offered to developers", () => {
  const stories = [
    { id: "TS-001", status: "done", roles: ["dev"], dependsOn: [] },
    { id: "TS-002", status: "todo", roles: ["dev"], dependsOn: ["TS-001"] },
    { id: "TS-003", status: "todo", roles: ["dev"], dependsOn: ["TS-002"] },
  ];
  assert.deepEqual(readyStories(stories).map((story) => story.id), ["TS-002"]);
});

test("factory verification builds Sites assets before testing the packaged worker", async () => {
  const source = await readFile(new URL("../scripts/factory.mjs", import.meta.url), "utf8");
  const build = source.indexOf('await run("npm", ["run", "build"])');
  const sites = source.indexOf('await run("npm", ["run", "test:sites"])');
  assert.ok(build >= 0 && sites >= 0 && build < sites);
});
