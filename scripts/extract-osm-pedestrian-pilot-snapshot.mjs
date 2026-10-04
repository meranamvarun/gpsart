import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";

const [inputPath, outputPath, checksumPath] = process.argv.slice(2);
if (!inputPath || !outputPath || !checksumPath) {
  throw new Error(
    "usage: node scripts/extract-osm-pedestrian-pilot-snapshot.mjs <raw-map.json> <snapshot.json> <snapshot.sha256>",
  );
}

const SOURCE_URL =
  "https://api.openstreetmap.org/api/0.6/map.json?bbox=77.5885,12.9720,77.5980,12.9805";
const RETRIEVED_AT = "2026-10-04T15:48:27.000Z";
const EXPECTED_RAW_SHA256 =
  "e2fed0d29ef5f4536bd8e2abdbdc7888c5834e78582fdfe4ea403c466967a30f";
const IMPLICIT_FOOT_HIGHWAYS = new Set(["footway", "pedestrian", "steps"]);
const ALLOWED_ACCESS = new Set(["yes", "permissive", "designated"]);
const DENIED_ACCESS = new Set(["no", "private"]);

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

function hasKnownWalkingAccess(tags) {
  if (DENIED_ACCESS.has(tags.foot)) return false;
  if (ALLOWED_ACCESS.has(tags.foot)) return true;
  if (tags["foot:conditional"] || tags["access:conditional"]) return false;
  if (DENIED_ACCESS.has(tags.access)) return false;
  if (ALLOWED_ACCESS.has(tags.access)) return true;
  return IMPLICIT_FOOT_HIGHWAYS.has(tags.highway);
}

function hasKnownWalkingDirection(tags) {
  const pathLike = IMPLICIT_FOOT_HIGHWAYS.has(tags.highway) || tags.highway === "path";
  if (tags["oneway:foot:conditional"] || (pathLike && tags["oneway:conditional"])) return false;
  if (tags["oneway:foot"]) {
    return ["yes", "1", "true", "-1", "reverse", "no", "0", "false"].includes(
      tags["oneway:foot"],
    );
  }
  return !pathLike || !tags.oneway;
}

function segmentMetres(left, right) {
  const radians = Math.PI / 180;
  const latitude1 = left.lat * radians;
  const latitude2 = right.lat * radians;
  const latitudeDelta = latitude2 - latitude1;
  const longitudeDelta = (right.lon - left.lon) * radians;
  const haversine =
    Math.sin(latitudeDelta / 2) ** 2 +
    Math.cos(latitude1) * Math.cos(latitude2) * Math.sin(longitudeDelta / 2) ** 2;
  return 12_742_017.6 * Math.asin(Math.sqrt(haversine));
}

const raw = await readFile(inputPath);
if (sha256(raw) !== EXPECTED_RAW_SHA256) {
  throw new Error("raw map bytes do not match the pinned capture checksum");
}
const source = JSON.parse(raw);
if (source.version !== "0.6" || !Array.isArray(source.elements)) {
  throw new Error("input is not an OSM API 0.6 JSON map response");
}

const restrictedWayIds = new Set(
  source.elements
    .filter((element) => element.type === "relation" && element.tags?.type === "restriction")
    .flatMap((relation) => relation.members ?? [])
    .filter((member) => member.type === "way")
    .map((member) => member.ref),
);
const sourceNodes = new Map(
  source.elements.filter((element) => element.type === "node").map((node) => [node.id, node]),
);
const duplicateCoordinates = new Set();
const coordinateOwners = new Map();
for (const node of sourceNodes.values()) {
  const key = `${node.lat},${node.lon}`;
  if (coordinateOwners.has(key)) {
    duplicateCoordinates.add(node.id);
    duplicateCoordinates.add(coordinateOwners.get(key));
  } else {
    coordinateOwners.set(key, node.id);
  }
}

const ways = source.elements
  .filter(
    (element) =>
      element.type === "way" &&
      element.tags?.highway &&
      !restrictedWayIds.has(element.id) &&
      hasKnownWalkingAccess(element.tags) &&
      hasKnownWalkingDirection(element.tags) &&
      element.nodes?.length >= 2 &&
      element.nodes.every((nodeId) => sourceNodes.has(nodeId) && !duplicateCoordinates.has(nodeId)) &&
      element.nodes
        .slice(1)
        .every((nodeId, index) =>
          segmentMetres(sourceNodes.get(element.nodes[index]), sourceNodes.get(nodeId)) >= 0.5,
        ),
  )
  .sort((left, right) => left.id - right.id);
ways.forEach(requireVersioned);
if (ways.length < 20) {
  throw new Error(`pedestrian pilot extraction is unexpectedly sparse: ${ways.length} ways`);
}

const selectedNodeIds = new Set(ways.flatMap((way) => way.nodes));
const nodes = [...selectedNodeIds]
  .map((id) => sourceNodes.get(id))
  .sort((left, right) => left.id - right.id);
nodes.forEach(requireVersioned);

const snapshot = {
  schema_version: 1,
  snapshot_id: "osm-cubbon-park-pedestrian-pilot-v2",
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
process.stdout.write(`${ways.length} ways, ${nodes.length} nodes, ${checksum}\n`);
