import { readFile, writeFile } from "node:fs/promises";

const [inputPath, outputPath] = process.argv.slice(2);
if (!inputPath || !outputPath) {
  throw new Error("usage: node scripts/render-osm-pilot-network.mjs <snapshot.json> <output.svg>");
}
const snapshot = JSON.parse(await readFile(inputPath, "utf8"));
const nodes = new Map(snapshot.nodes.map((node) => [node.id, node]));
const minLatitude = Math.min(...snapshot.nodes.map((node) => node.latitude_e7));
const maxLatitude = Math.max(...snapshot.nodes.map((node) => node.latitude_e7));
const minLongitude = Math.min(...snapshot.nodes.map((node) => node.longitude_e7));
const maxLongitude = Math.max(...snapshot.nodes.map((node) => node.longitude_e7));
const latitudeScale = Math.cos(((minLatitude + maxLatitude) / 20_000_000) * Math.PI / 180);
const width = (maxLongitude - minLongitude) * latitudeScale;
const height = maxLatitude - minLatitude;
const scale = Math.min(1_120 / width, 820 / height);
const point = (node) =>
  `${40 + (node.longitude_e7 - minLongitude) * latitudeScale * scale},${40 + (maxLatitude - node.latitude_e7) * scale}`;
const ways = snapshot.ways
  .map(
    (way) =>
      `<polyline points="${way.nodes.map((id) => point(nodes.get(id))).join(" ")}" fill="none" stroke="#40566c" stroke-width="1.5"/>`,
  )
  .join("");
await writeFile(
  outputPath,
  `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="900" viewBox="0 0 1200 900"><rect width="1200" height="900" fill="#f7f4ee"/>${ways}</svg>\n`,
);
