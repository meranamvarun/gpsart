import { createHash } from "node:crypto";
import { writeFile } from "node:fs/promises";

const [outputPath, requestedSourceUrl] = process.argv.slice(2);
if (!outputPath) {
  throw new Error(
    "usage: node scripts/capture-osm-pilot-raw.mjs <raw-map.json> [osm-map-url]",
  );
}

const sourceUrl = requestedSourceUrl ??
  "https://api.openstreetmap.org/api/0.6/map.json?bbox=77.5885,12.9720,77.5980,12.9805";
if (!sourceUrl.startsWith("https://api.openstreetmap.org/api/0.6/map.json?bbox=")) {
  throw new Error("source URL must be an OpenStreetMap API 0.6 bounding-box request");
}
const response = await fetch(sourceUrl, {
  headers: { "user-agent": "gpsart-pilot-snapshot/2.0" },
});
if (!response.ok) {
  throw new Error(`OSM API returned ${response.status}`);
}
const raw = Buffer.from(await response.arrayBuffer());
const sha256 = createHash("sha256").update(raw).digest("hex");
const retrievedAt = new Date(response.headers.get("date") ?? Date.now()).toISOString();
await writeFile(outputPath, raw);
process.stdout.write(`${sha256} ${retrievedAt}\n`);
