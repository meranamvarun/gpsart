import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import { scanProductionBundle } from "../scripts/scan-production-bundle.mjs";

test("production bundle scan rejects prototype success labels and fixture identifiers", async () => {
  const root = await mkdtemp(path.join(tmpdir(), "gpsart-bundle-"));
  try {
    await mkdir(path.join(root, "assets"));
    await writeFile(path.join(root, "assets", "safe.js"), "const state = 'No route geometry is displayed';");
    assert.deepEqual(await scanProductionBundle(root), []);

    await writeFile(path.join(root, "assets", "unsafe.js"), "const id='40000000-0000-4000-8000-000000000001'; const label='Sample GPX';");
    const findings = await scanProductionBundle(root);
    assert.ok(findings.some((finding) => finding.includes("route snapshot fixture id")));
    assert.ok(findings.some((finding) => finding.includes("sample GPX action")));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
