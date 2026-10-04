import { readFile } from "node:fs/promises";

const [snapshotPath, templatePath, startIdText, toleranceText, minimumScaleText] = process.argv.slice(2);
if (!snapshotPath || !templatePath || !startIdText || !toleranceText) {
  throw new Error(
    "usage: node scripts/find-osm-template-placement.mjs <snapshot.json> <template.json> <start-node-id> <tolerance-metres>",
  );
}

const snapshot = JSON.parse(await readFile(snapshotPath, "utf8"));
const template = JSON.parse(await readFile(templatePath, "utf8"));
const startId = Number(startIdText);
const tolerance = Number(toleranceText);
const minimumScale = minimumScaleText ? Number(minimumScaleText) : 40;
const nodesById = new Map(snapshot.nodes.map((node) => [node.id, node]));
const start = nodesById.get(startId);
if (!start || !Number.isFinite(tolerance) || tolerance <= 0) {
  throw new Error("start node or tolerance is invalid");
}

const neighbours = new Map(snapshot.nodes.map((node) => [node.id, []]));
const adjacency = new Map(snapshot.nodes.map((node) => [node.id, []]));
function distanceBetweenIds(leftId, rightId) {
  const left = nodesById.get(leftId);
  const right = nodesById.get(rightId);
  const north = ((right.latitude_e7 - left.latitude_e7) / 10_000_000) * 111_320;
  const latitude = ((left.latitude_e7 + right.latitude_e7) / 20_000_000) * Math.PI / 180;
  const east =
    ((right.longitude_e7 - left.longitude_e7) / 10_000_000) * 111_320 * Math.cos(latitude);
  return Math.hypot(east, north);
}
for (const way of snapshot.ways) {
  const footDirection = way.tags["oneway:foot"];
  for (let index = 1; index < way.nodes.length; index += 1) {
    const left = way.nodes[index - 1];
    const right = way.nodes[index];
    const distance = distanceBetweenIds(left, right);
    neighbours.get(left).push(right);
    neighbours.get(right).push(left);
    if (!["-1", "reverse"].includes(footDirection)) {
      adjacency.get(left).push({ id: right, distance });
    }
    if (!["yes", "1", "true"].includes(footDirection)) {
      adjacency.get(right).push({ id: left, distance });
    }
  }
}
const reachable = new Set([startId]);
const queue = [startId];
for (let index = 0; index < queue.length; index += 1) {
  for (const next of neighbours.get(queue[index])) {
    if (!reachable.has(next)) {
      reachable.add(next);
      queue.push(next);
    }
  }
}

const metresPerDegree = 111_320;
const longitudeScale = metresPerDegree * Math.cos((start.latitude_e7 / 10_000_000) * Math.PI / 180);
const localNodes = snapshot.nodes
  .filter((node) => reachable.has(node.id))
  .map((node) => ({
    id: node.id,
    east: ((node.longitude_e7 - start.longitude_e7) / 10_000_000) * longitudeScale,
    north: ((node.latitude_e7 - start.latitude_e7) / 10_000_000) * metresPerDegree,
  }));
const localById = new Map(localNodes.map((node) => [node.id, node]));
const grid = new Map();
for (const node of localNodes) {
  const key = `${Math.floor(node.east / tolerance)},${Math.floor(node.north / tolerance)}`;
  const bucket = grid.get(key) ?? [];
  bucket.push(node);
  grid.set(key, bucket);
}
const traversal = [];
let previous;
for (let partIndex = 0; partIndex < template.strokes.length; partIndex += 1) {
  const parts = [template.strokes[partIndex]];
  if (partIndex < template.transitions.length) parts.push(template.transitions[partIndex]);
  for (const part of parts) {
    for (const point of part.points) {
      const key = `${point.x},${point.y}`;
      if (key !== previous) traversal.push(point);
      previous = key;
    }
  }
}

function nearest(east, north) {
  let best;
  const cellEast = Math.floor(east / tolerance);
  const cellNorth = Math.floor(north / tolerance);
  for (let eastOffset = -1; eastOffset <= 1; eastOffset += 1) {
    for (let northOffset = -1; northOffset <= 1; northOffset += 1) {
      const bucket = grid.get(`${cellEast + eastOffset},${cellNorth + northOffset}`) ?? [];
      for (const node of bucket) {
        const distance = Math.hypot(node.east - east, node.north - north);
        if (!best || distance < best.distance || (distance === best.distance && node.id < best.id)) {
          best = { id: node.id, distance };
        }
      }
    }
  }
  return best ?? { id: 0, distance: Infinity };
}

const rotations = template.id === "HI"
  ? Array.from({ length: 11 }, (_, index) => index * 3 - 15)
  : Array.from({ length: 25 }, (_, index) => index * 15 - 180);
const matches = [];
for (let east = -400; east <= 400; east += 10) {
  for (let north = -400; north <= 400; north += 10) {
    for (let scale = minimumScale; scale <= 400; scale += 10) {
      for (const rotation of rotations) {
        const radians = rotation * Math.PI / 180;
        const sin = Math.sin(radians);
        const cos = Math.cos(radians);
        const selected = [];
        let totalError = 0;
        let maximumError = 0;
        for (const point of traversal) {
          const localEast = (point.x - 0.5) * scale;
          const localNorth = (0.5 - point.y) * scale;
          const targetEast = localEast * cos - localNorth * sin + east;
          const targetNorth = localEast * sin + localNorth * cos + north;
          const candidate = nearest(targetEast, targetNorth);
          if (candidate.distance > tolerance || selected.at(-1) === candidate.id) {
            maximumError = Infinity;
            break;
          }
          selected.push(candidate.id);
          totalError += candidate.distance;
          maximumError = Math.max(maximumError, candidate.distance);
        }
        const distinctCount = new Set(selected).size;
        const expectedDistinct =
          template.id === "heart" && selected[0] === selected.at(-1)
            ? selected.length - 1
            : selected.length;
        if (Number.isFinite(maximumError) && distinctCount === expectedDistinct) {
          matches.push({ east, north, scale, rotation, totalError, maximumError, selected });
        }
      }
    }
  }
}
matches.sort(
  (left, right) =>
    left.maximumError - right.maximumError ||
    left.totalError - right.totalError ||
    left.scale - right.scale ||
    left.rotation - right.rotation,
);

class MinimumHeap {
  items = [];

  push(item) {
    this.items.push(item);
    let index = this.items.length - 1;
    while (index > 0) {
      const parent = Math.floor((index - 1) / 2);
      if (this.items[parent].distance <= item.distance) break;
      this.items[index] = this.items[parent];
      index = parent;
    }
    this.items[index] = item;
  }

  pop() {
    if (this.items.length === 0) return undefined;
    const first = this.items[0];
    const last = this.items.pop();
    if (this.items.length > 0) {
      let index = 0;
      while (true) {
        const left = index * 2 + 1;
        const right = left + 1;
        if (left >= this.items.length) break;
        const child =
          right < this.items.length && this.items[right].distance < this.items[left].distance
            ? right
            : left;
        if (this.items[child].distance >= last.distance) break;
        this.items[index] = this.items[child];
        index = child;
      }
      this.items[index] = last;
    }
    return first;
  }
}

const pathCache = new Map();
function shortestPath(from, to) {
  const key = `${from}>${to}`;
  if (pathCache.has(key)) return pathCache.get(key);
  const distances = new Map([[from, 0]]);
  const previous = new Map();
  const heap = new MinimumHeap();
  heap.push({ id: from, distance: 0 });
  while (heap.items.length > 0) {
    const current = heap.pop();
    if (current.distance !== distances.get(current.id)) continue;
    if (current.id === to) break;
    for (const edge of adjacency.get(current.id)) {
      const nextDistance = current.distance + edge.distance;
      if (nextDistance < (distances.get(edge.id) ?? Infinity)) {
        distances.set(edge.id, nextDistance);
        previous.set(edge.id, current.id);
        heap.push({ id: edge.id, distance: nextDistance });
      }
    }
  }
  if (!distances.has(to)) {
    pathCache.set(key, undefined);
    return undefined;
  }
  const path = [to];
  while (path.at(-1) !== from) path.push(previous.get(path.at(-1)));
  path.reverse();
  const result = { distance: distances.get(to), path };
  pathCache.set(key, result);
  return result;
}

function pointToPolyline(point, polyline) {
  let closest = Infinity;
  for (let index = 1; index < polyline.length; index += 1) {
    const left = polyline[index - 1];
    const right = polyline[index];
    const dx = right.east - left.east;
    const dy = right.north - left.north;
    const denominator = dx * dx + dy * dy;
    const factor = denominator === 0
      ? 0
      : Math.max(
          0,
          Math.min(1, ((point.east - left.east) * dx + (point.north - left.north) * dy) / denominator),
        );
    closest = Math.min(
      closest,
      Math.hypot(point.east - (left.east + factor * dx), point.north - (left.north + factor * dy)),
    );
  }
  return closest;
}

function scoreRoute(match) {
  let best;
  for (const candidateStart of neighbours.get(match.selected[0])) {
    if (candidateStart === match.selected[0] || candidateStart === match.selected.at(-1)) continue;
    const ordered = [candidateStart, ...match.selected, candidateStart];
    const traceIds = [];
    const routeSegments = [];
    let routeDistance = 0;
    let repeatedDistance = 0;
    const seenEdges = new Set();
    let valid = true;
    for (let index = 1; index < ordered.length; index += 1) {
      const segment = shortestPath(ordered[index - 1], ordered[index]);
      if (!segment) {
        valid = false;
        break;
      }
      routeDistance += segment.distance;
      routeSegments.push(segment.path);
      for (let pathIndex = 1; pathIndex < segment.path.length; pathIndex += 1) {
        const left = segment.path[pathIndex - 1];
        const right = segment.path[pathIndex];
        const edgeKey = left < right ? `${left}:${right}` : `${right}:${left}`;
        const edgeDistance = distanceBetweenIds(left, right);
        if (seenEdges.has(edgeKey)) repeatedDistance += edgeDistance;
        else seenEdges.add(edgeKey);
      }
      traceIds.push(...segment.path.slice(index === 1 ? 0 : 1));
    }
    if (!valid) continue;
    const trace = traceIds.map((id) => localById.get(id));
    const design = match.selected.map((id) => localById.get(id));
    const traceError = Math.max(
      ...trace.map((point) => pointToPolyline(point, design)),
      ...design.map((point) => pointToPolyline(point, trace)),
    );
    const directDistance = ordered.slice(1).reduce(
      (sum, id, index) => sum + distanceBetweenIds(ordered[index], id),
      0,
    );
    const repeatedRatio = repeatedDistance / routeDistance;
    const artworkSegmentIndexes = template.id === "HI"
      ? [1, 3, 5, 7]
      : Array.from({ length: match.selected.length - 1 }, (_, index) => index + 1);
    const artworkError = Math.max(
      ...artworkSegmentIndexes.flatMap((segmentIndex) => {
        const from = localById.get(ordered[segmentIndex]);
        const to = localById.get(ordered[segmentIndex + 1]);
        return routeSegments[segmentIndex].map((id) =>
          pointToPolyline(localById.get(id), [from, to]),
        );
      }),
    );
    const routeScore =
      (artworkError / match.scale) * 200 +
      (traceError / match.scale) * 25 +
      (match.maximumError / match.scale) * 50 +
      repeatedRatio * 50 +
      routeDistance / Math.max(directDistance, 1);
    if (!best || routeScore < best.routeScore) {
      const startPoint = localById.get(candidateStart);
      best = {
        startNodeId: candidateStart,
        configEast: Math.round(match.east - startPoint.east),
        configNorth: Math.round(match.north - startPoint.north),
        routeDistance,
        repeatedRatio,
        artworkError,
        traceError,
        routeScore,
      };
    }
  }
  return best ? { ...match, ...best } : undefined;
}

const routedMatches = matches
  .slice()
  .sort(
    (left, right) =>
      left.maximumError / left.scale - right.maximumError / right.scale ||
      left.totalError / left.scale - right.totalError / right.scale,
  )
  .slice(0, 2_000)
  .map(scoreRoute)
  .filter(Boolean)
  .sort((left, right) => left.routeScore - right.routeScore || left.maximumError - right.maximumError);
process.stdout.write(
  `${JSON.stringify({ reachableNodes: reachable.size, matches: routedMatches.slice(0, 20) }, null, 2)}\n`,
);
