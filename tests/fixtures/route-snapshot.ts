import { decodeRouteSnapshot } from "../../src/domain/route-snapshot.ts";

export function routeSnapshotPayload() {
  return {
    id: "40000000-0000-4000-8000-000000000001",
    generation_job_id: "30000000-0000-4000-8000-000000000001",
    candidate_version: 1,
    request_revision: 1,
    geometry: { type: "LineString", coordinates: [[77.6, 12.9], [77.601, 12.901], [77.6, 12.9]] },
    traversals: [
      { edge_id: "art", class: "artwork", direction: "forward", begin_geometry_index: 0, end_geometry_index: 1, distance_metres: 600, retraced: false },
      { edge_id: "connector", class: "return", direction: "reverse", begin_geometry_index: 1, end_geometry_index: 2, distance_metres: 400, retraced: true },
    ],
    distances: { artwork_metres: 600, connector_metres: 400, total_metres: 1000, repeated_metres: 400, repeated_included_in_total: true, equality_tolerance_metres: 1 },
    quality_band: "approximate_match",
    qualifications: { legality_version: "candidate-legality-v1", route_use_advisory_version: "route-use-advisory-v1", access_verified: true, direction_verified: true, turns_verified: true },
    versions: { snapshot: "route-candidate-snapshot-v1", request: "route-request-v1", profile: "walking-v1", template_id: "heart", template: 1, template_hash: "f".repeat(64), algorithm: "algorithm-v1", routing_data: "map-v1", routing_policy: "osm-pedestrian-policy-v2", candidate: 1 },
    generated_at_epoch_seconds: 1800000000,
    provenance: { routing_provider: "controlled-local-osm", attribution: "OpenStreetMap contributors" },
    start: [77.6, 12.9],
    finish: [77.6, 12.9],
    duration_seconds: 720,
    instructions: [
      { kind: "depart", geometry_index: 0 },
      { kind: "via", geometry_index: 1 },
      { kind: "arrive", geometry_index: 2 },
    ],
    direction_markers: [
      { traversal_index: 0, coordinate: [77.6, 12.9], direction: "forward" },
      { traversal_index: 1, coordinate: [77.601, 12.901], direction: "reverse" },
    ],
  };
}

export function routeSnapshot() {
  const decoded = decodeRouteSnapshot(routeSnapshotPayload());
  if (!decoded) throw new Error("route snapshot fixture must remain valid");
  return decoded;
}
