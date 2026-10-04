import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";

const [inputPath, outputPath, checksumPath] = process.argv.slice(2);
if (!inputPath || !outputPath || !checksumPath) {
  throw new Error(
    "usage: node scripts/extract-osm-pilot-snapshot.mjs <raw-map.json> <snapshot.json> <snapshot.sha256>",
  );
}

const SELECTED_WAY_IDS = new Set([
  46_728_546,
  52_032_471,
  369_295_564,
  399_001_081,
  508_197_004,
]);
const SOURCE_URL =
  "https://api.openstreetmap.org/api/0.6/map.json?bbox=77.5885,12.9720,77.5980,12.9805";
const RETRIEVED_AT = "2026-10-04T03:11:34Z";
const EXPECTED_RAW_SHA256 =
  "3828e4099d571653e7cd1ee05caa73dadb5ec34ad99b80906cc24a0be3a5c5da";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function requireVersioned(element) {
  if (!Number.isSafeInteger(element.id) || !Number.isSafeInteger(element.version) || element.version < 1) {
    throw new Error(`OSM ${element.type} is missing an immutable id/version`);
  }
  if (typeof element.timestamp !== "string" || !element.timestamp.endsWith("Z")) {
    throw new Error(`OSM ${element.type}/${element.id} is missing a UTC timestamp`);
  }
}

const raw = await readFile(inputPath);
if (sha256(raw) !== EXPECTED_RAW_SHA256) {
  throw new Error("raw map bytes do not match the v1 capture checksum");
}
const source = JSON.parse(raw);
if (source.version !== "0.6" || !Array.isArray(source.elements)) {
  throw new Error("input is not an OSM API 0.6 JSON map response");
}

const ways = source.elements
  .filter((element) => element.type === "way" && SELECTED_WAY_IDS.has(element.id))
  .sort((left, right) => left.id - right.id);
if (ways.length !== SELECTED_WAY_IDS.size) {
  throw new Error("input does not contain every selected pilot way");
}
ways.forEach(requireVersioned);

const selectedNodeIds = new Set(ways.flatMap((way) => way.nodes));
const nodes = source.elements
  .filter((element) => element.type === "node" && selectedNodeIds.has(element.id))
  .sort((left, right) => left.id - right.id);
if (nodes.length !== selectedNodeIds.size) {
  throw new Error("input does not contain every node referenced by a selected pilot way");
}
nodes.forEach(requireVersioned);

const touchingRelations = source.elements.filter(
  (element) =>
    element.type === "relation" &&
    element.members?.some(
      (member) => member.type === "way" && SELECTED_WAY_IDS.has(member.ref),
    ),
);
if (touchingRelations.length !== 0) {
  throw new Error(
    `selected ways participate in relations that must not be omitted: ${touchingRelations.map(({ id }) => id).join(",")}`,
  );
}

const snapshot = {
  schema_version: 1,
  snapshot_id: "osm-cubbon-park-pilot-v1",
  source: {
    api: "OpenStreetMap API 0.6",
    url: SOURCE_URL,
    retrieved_at: RETRIEVED_AT,
    raw_sha256: EXPECTED_RAW_SHA256,
    attribution: "OpenStreetMap contributors",
    license: "ODbL-1.0",
  },
  nodes: nodes.map(({ id, version, timestamp, lat, lon }) => ({
    id,
    version,
    timestamp,
    latitude_e7: Math.round(lat * 10_000_000),
    longitude_e7: Math.round(lon * 10_000_000),
  })),
  ways: ways.map(({ id, version, timestamp, nodes: wayNodes, tags }) => ({
    id,
    version,
    timestamp,
    nodes: wayNodes,
    tags,
  })),
  forbidden_turns: [],
};

const rendered = Buffer.from(`${JSON.stringify(snapshot, null, 2)}\n`);
const checksum = sha256(rendered);
await writeFile(outputPath, rendered);
await writeFile(checksumPath, `${checksum}\n`);
