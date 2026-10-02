import { readFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const ROOT = new URL("../", import.meta.url);
const storiesUrl = new URL("../factory/stories.json", import.meta.url);
const incidentsUrl = new URL("../factory/incidents.jsonl", import.meta.url);
const VALID_STATUSES = new Set(["todo", "claimed", "in_review", "done", "blocked"]);
const roles = new Set(["coordinator", "dev", "qa", "maintainer"]);

export async function loadFactory() {
  return JSON.parse(await readFile(storiesUrl, "utf8"));
}

export function validateStories({ stories }) {
  const errors = [];
  const ids = new Set();
  for (const story of stories) {
    if (!/^TS-\d{3}$/.test(story.id) || ids.has(story.id)) errors.push(`invalid or duplicate story id: ${story.id}`);
    ids.add(story.id);
    if (!VALID_STATUSES.has(story.status)) errors.push(`${story.id}: invalid status ${story.status}`);
    if (!Array.isArray(story.dependsOn) || !Array.isArray(story.roles) || !Array.isArray(story.acceptanceEvidence)) errors.push(`${story.id}: required arrays are missing`);
    if (!story.roles?.every((role) => roles.has(role))) errors.push(`${story.id}: invalid role`);
    if (story.status === "done" && (!story.qa?.reviewer || !story.qa?.verifiedAt || story.acceptanceEvidence.length === 0)) {
      errors.push(`${story.id}: done stories require QA reviewer, verification date, and evidence`);
    }
  }
  for (const story of stories) for (const dep of story.dependsOn ?? []) if (!ids.has(dep)) errors.push(`${story.id}: unknown dependency ${dep}`);
  return errors;
}

export function readyStories(stories, role = "dev") {
  return stories.filter((story) => story.status === "todo" && story.roles.includes(role) && story.dependsOn.every((id) => stories.find((item) => item.id === id)?.status === "done"));
}

async function openIncidents() {
  const text = await readFile(incidentsUrl, "utf8");
  const issues = [];
  for (const [index, line] of text.split(/\r?\n/).entries()) {
    if (!line.trim() || line.trimStart().startsWith("#")) continue;
    try {
      const incident = JSON.parse(line);
      if (incident.status !== "closed") issues.push(`incident line ${index + 1}: ${incident.id ?? "missing id"} is not closed`);
      if (!incident.preventionCheck) issues.push(`incident line ${index + 1}: preventionCheck is required`);
    } catch { issues.push(`incident line ${index + 1}: invalid JSON`); }
  }
  return issues;
}

function run(command, args) {
  return new Promise((resolve) => {
    const child = spawn(command, args, { stdio: "inherit", shell: process.platform === "win32" });
    child.on("exit", (code) => resolve(code ?? 1));
  });
}

async function main() {
  const [command = "status", ...args] = process.argv.slice(2);
  const factory = await loadFactory();
  const errors = validateStories(factory);
  if (command === "next") {
    const role = args[args.indexOf("--role") + 1] ?? "dev";
    if (!roles.has(role)) throw new Error(`unknown role: ${role}`);
    const ready = readyStories(factory.stories, role);
    console.log(ready.length ? ready.map((s) => `${s.id} [stage ${s.stage}] ${s.title}`).join("\n") : `No ready stories for ${role}.`);
    return;
  }
  if (command === "status") {
    for (const stage of [...new Set(factory.stories.map((s) => s.stage))]) {
      const entries = factory.stories.filter((s) => s.stage === stage);
      console.log(`Stage ${stage}: ${entries.map((s) => `${s.id}=${s.status}`).join(", ")}`);
    }
    return;
  }
  if (command !== "verify") throw new Error(`unknown factory command: ${command}`);
  errors.push(...await openIncidents());
  if (errors.length) throw new Error(`Factory validation failed:\n${errors.map((e) => `- ${e}`).join("\n")}`);
  const factoryTestCode = await run("npm", ["run", "test:factory"]);
  const unitTestCode = await run("npm", ["run", "test:unit"]);
  const visualShellTestCode = await run("npm", ["run", "test:visual-shell"]);
  const testCode = await run("npm", ["run", "test:sites"]);
  const buildCode = await run("npm", ["run", "build"]);
  if (factoryTestCode || unitTestCode || visualShellTestCode || testCode || buildCode) process.exitCode = 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
