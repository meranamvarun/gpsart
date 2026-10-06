import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { downloadRouteSnapshotGpx, exportRouteSnapshotGpx, routeExportFilename } from "../src/domain/gpx-export.ts";
import { routeSnapshot } from "./fixtures/route-snapshot.ts";

test("offline export serializes every exact point in order and matches the server fixture", async () => {
  const snapshot = routeSnapshot();
  const serverEquivalent = {
    ...snapshot,
    trace: [
      { longitude: 77.6, latitude: 12.9 },
      { longitude: 77.601, latitude: 12.901 },
      { longitude: 77.6, latitude: 12.901 },
      { longitude: 77.601, latitude: 12.9 },
      { longitude: 77.6, latitude: 12.9 },
    ],
  };
  const expected = await readFile(new URL("./fixtures/route-export.gpx", import.meta.url), "utf8");
  const exported = exportRouteSnapshotGpx(serverEquivalent);
  assert.equal(exported, expected);
  assert.deepEqual(
    [...exported.matchAll(/<trkpt lat="([^"]+)" lon="([^"]+)">/g)].map((match) => ({
      latitude: Number(match[1]),
      longitude: Number(match[2]),
    })),
    serverEquivalent.trace,
  );
  assert.equal(exported.match(/<trkseg>/g)?.length, 1);
  assert.doesNotMatch(exported, /<(time|speed|hr|extensions)>\b/i);
  assert.match(exported, /planned route/i);
  assert.match(exported, /connectors and retraced sections/i);
});

test("offline export escapes metadata and creates a predictable sanitized filename", () => {
  const snapshot = routeSnapshot();
  const hostile = {
    ...snapshot,
    versions: { ...snapshot.versions, templateId: 'HI & <script> / "route"' },
  };
  const exported = exportRouteSnapshotGpx(hostile);
  assert.match(exported, /HI &amp; &lt;script&gt; \/ &quot;route&quot;@1/);
  assert.doesNotMatch(exported, /<script>/);
  assert.equal(routeExportFilename(hostile), `gpsart-hi-script-route-${snapshot.id}-v1.gpx`);
});

test("offline browser download uses GPX media type and sanitized attachment name", () => {
  const snapshot = routeSnapshot();
  let clicked = false;
  const anchor = { href: "", download: "", rel: "", click: () => { clicked = true; } };
  const documentRoot = { createElement: () => anchor } as unknown as Document;
  downloadRouteSnapshotGpx(snapshot, documentRoot);
  assert.equal(clicked, true);
  assert.equal(anchor.download, routeExportFilename(snapshot));
  assert.equal(anchor.rel, "noopener");
  assert.match(anchor.href, /^blob:/);
});
