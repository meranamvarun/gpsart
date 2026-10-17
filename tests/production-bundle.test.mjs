import assert from "node:assert/strict";
import { access, mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
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

    await writeFile(path.join(root, "assets", "unsupported.js"), "const auth='Continue with Google'; const search='gpsart:search-selection'; const city='explore your city'; const place='choose a search result';");
    const unsupported = await scanProductionBundle(root);
    assert.ok(unsupported.some((finding) => finding.includes("unavailable Google sign-in surface")));
    assert.ok(unsupported.some((finding) => finding.includes("unavailable place-search bridge")));
    assert.ok(unsupported.some((finding) => finding.includes("unsupported citywide claim")));
    assert.ok(unsupported.some((finding) => finding.includes("unsupported place-search copy")));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("production pages declare a build-owned SVG favicon", async () => {
  const pages = ["index.html", "login.html", "routes.html", "saved.html", "screens.html"];

  for (const page of pages) {
    const html = await readFile(new URL(`../${page}`, import.meta.url), "utf8");
    assert.match(
      html,
      /<link rel="icon" href="\/favicon\.svg" type="image\/svg\+xml">/,
      `${page} must declare the favicon and its media type`,
    );
  }

  const favicon = new URL("../public/favicon.svg", import.meta.url);
  await access(favicon);
  assert.match(await readFile(favicon, "utf8"), /^<svg[^>]+xmlns="http:\/\/www\.w3\.org\/2000\/svg"/);
});
