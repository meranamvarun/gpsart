#!/usr/bin/env node
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const forbidden = [
  { label: "route snapshot fixture id", pattern: /40000000-0000-4000-8000-[0-9a-f]{12}/i },
  { label: "sample GPX action", pattern: /sample\s+gpx/i },
  { label: "sample route label", pattern: /sample\s+route/i },
  { label: "demo route label", pattern: /demo\s+route/i },
  { label: "prototype success label", pattern: /prototype\s+success/i },
  { label: "unavailable Google sign-in surface", pattern: /continue\s+with\s+google/i },
  { label: "unavailable place-search bridge", pattern: /gpsart:search-selection/i },
  { label: "unsupported citywide claim", pattern: /explore\s+your\s+city/i },
  { label: "unsupported place-search copy", pattern: /choose\s+a\s+search\s+result/i },
];
const textExtensions = new Set([".css", ".html", ".js", ".json", ".map", ".txt"]);

async function filesUnder(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const nested = await Promise.all(entries.map(async (entry) => {
    const resolved = path.join(root, entry.name);
    return entry.isDirectory() ? filesUnder(resolved) : [resolved];
  }));
  return nested.flat();
}

export async function scanProductionBundle(root) {
  const findings = [];
  for (const file of await filesUnder(root)) {
    if (!textExtensions.has(path.extname(file).toLowerCase())) continue;
    const contents = await readFile(file, "utf8");
    for (const { label, pattern } of forbidden) {
      if (pattern.test(contents)) findings.push(`${path.relative(root, file)}: ${label}`);
    }
  }
  return findings;
}

async function main() {
  const root = path.resolve(process.argv[2] ?? path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "dist", "client"));
  const findings = await scanProductionBundle(root);
  if (findings.length) throw new Error(`Production bundle contains forbidden prototype material:\n${findings.map((finding) => `- ${finding}`).join("\n")}`);
  console.log(`Production bundle scan passed: ${root}`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
