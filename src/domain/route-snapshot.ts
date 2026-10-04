export interface Position { readonly latitude: number; readonly longitude: number; }

export type TraversalClass = "artwork" | "connector" | "access" | "return";
export type TraversalDirection = "forward" | "reverse";
export type QualityBand = "strong_match" | "approximate_match";

export interface RouteTraversal {
  readonly edgeId: string;
  readonly class: TraversalClass;
  readonly direction: TraversalDirection;
  readonly beginGeometryIndex: number;
  readonly endGeometryIndex: number;
  readonly distanceMetres: number;
  readonly retraced: boolean;
}

export interface SnapshotVersions {
  readonly snapshot: "route-candidate-snapshot-v1";
  readonly request: string;
  readonly profile: string;
  readonly templateId: string;
  readonly template: number;
  readonly templateHash: string;
  readonly algorithm: string;
  readonly routingData: string;
  readonly routingPolicy: string;
  readonly candidate: number;
}

export interface CandidateProvenance {
  readonly routingProvider: string;
  readonly attribution?: string;
}

export interface CandidateQualifications {
  readonly legalityVersion: "candidate-legality-v1";
  readonly routeUseAdvisoryVersion: "route-use-advisory-v1";
  readonly accessVerified: true;
  readonly directionVerified: true;
  readonly turnsVerified: true;
}

export interface DirectionMarker {
  readonly traversalIndex: number;
  readonly position: Position;
  readonly direction: TraversalDirection;
}

export interface RouteInstruction {
  readonly kind: "depart" | "via" | "arrive" | "provider";
  readonly geometryIndex: number;
  readonly providerCode?: number;
}

export interface RouteSnapshot {
  readonly id: string;
  readonly generationJobId: string;
  readonly version: number;
  readonly requestRevision: number;
  readonly trace: readonly Position[];
  readonly traversals: readonly RouteTraversal[];
  readonly artworkMetres: number;
  readonly connectorMetres: number;
  readonly totalMetres: number;
  readonly repeatedMetres: number;
  readonly repeatedIncludedInTotal: true;
  readonly distanceEqualityToleranceMetres: 1;
  readonly qualityBand: QualityBand;
  readonly qualifications: CandidateQualifications;
  readonly versions: SnapshotVersions;
  readonly generatedAtEpochSeconds: number;
  readonly provenance: CandidateProvenance;
  readonly start: Position;
  readonly finish: Position;
  readonly durationSeconds: number;
  readonly instructions: readonly RouteInstruction[];
  readonly directionMarkers: readonly DirectionMarker[];
}

const uuidV4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const hash = /^[0-9a-f]{64}$/;
const traversalClasses: readonly TraversalClass[] = ["artwork", "connector", "access", "return"];
const directions: readonly TraversalDirection[] = ["forward", "reverse"];
const qualityBands: readonly QualityBand[] = ["strong_match", "approximate_match"];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function keysAre(value: Record<string, unknown>, required: readonly string[], optional: readonly string[] = []): boolean {
  const keys = Object.keys(value);
  return required.every((key) => keys.includes(key)) && keys.every((key) => required.includes(key) || optional.includes(key));
}

function positiveInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) > 0;
}

function nonNegativeInteger(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) >= 0;
}

function coordinate(value: unknown): readonly [number, number] | null {
  if (!Array.isArray(value) || value.length !== 2) return null;
  const [longitude, latitude] = value;
  if (typeof longitude !== "number" || typeof latitude !== "number" || !Number.isFinite(longitude) || !Number.isFinite(latitude)) return null;
  if (longitude < -180 || longitude > 180 || latitude < -90 || latitude > 90) return null;
  return [longitude, latitude];
}

function position(value: unknown): Position | null {
  const point = coordinate(value);
  return point ? Object.freeze({ longitude: point[0], latitude: point[1] }) : null;
}

function samePosition(left: Position, right: Position): boolean {
  return left.longitude === right.longitude && left.latitude === right.latitude;
}

function decodeTraversal(value: unknown): RouteTraversal | null {
  if (!isRecord(value) || !keysAre(value, ["edge_id", "class", "direction", "begin_geometry_index", "end_geometry_index", "distance_metres", "retraced"])) return null;
  if (typeof value.edge_id !== "string" || value.edge_id.length === 0) return null;
  if (typeof value.class !== "string" || !traversalClasses.includes(value.class as TraversalClass) || typeof value.direction !== "string" || !directions.includes(value.direction as TraversalDirection)) return null;
  if (!nonNegativeInteger(value.begin_geometry_index) || !positiveInteger(value.end_geometry_index) || value.end_geometry_index <= value.begin_geometry_index || !positiveInteger(value.distance_metres) || typeof value.retraced !== "boolean") return null;
  return Object.freeze({
    edgeId: value.edge_id,
    class: value.class as TraversalClass,
    direction: value.direction as TraversalDirection,
    beginGeometryIndex: value.begin_geometry_index,
    endGeometryIndex: value.end_geometry_index,
    distanceMetres: value.distance_metres,
    retraced: value.retraced,
  });
}

export function decodeRouteSnapshot(value: unknown): RouteSnapshot | null {
  if (!isRecord(value) || !keysAre(value, [
    "id", "generation_job_id", "candidate_version", "request_revision", "geometry", "traversals",
    "distances", "quality_band", "qualifications", "versions", "generated_at_epoch_seconds",
    "provenance", "start", "finish", "duration_seconds", "instructions", "direction_markers",
  ])) return null;
  if (typeof value.id !== "string" || !uuidV4.test(value.id) || typeof value.generation_job_id !== "string" || !uuidV4.test(value.generation_job_id)) return null;
  if (!positiveInteger(value.candidate_version) || !positiveInteger(value.request_revision) || !positiveInteger(value.generated_at_epoch_seconds)) return null;

  if (!isRecord(value.geometry) || !keysAre(value.geometry, ["type", "coordinates"]) || value.geometry.type !== "LineString" || !Array.isArray(value.geometry.coordinates)) return null;
  const coordinates = value.geometry.coordinates.map(coordinate);
  if (coordinates.length < 2 || coordinates.some((point) => point === null)) return null;
  const trace = (coordinates as (readonly [number, number])[]).map((point) => Object.freeze({ longitude: point[0], latitude: point[1] }));

  if (!Array.isArray(value.traversals)) return null;
  const traversals = value.traversals.map(decodeTraversal);
  if (traversals.length === 0 || traversals.some((item) => item === null)) return null;
  const validTraversals = traversals as RouteTraversal[];
  let expectedIndex = 0;
  let artwork = 0;
  let connector = 0;
  let total = 0;
  let repeated = 0;
  for (const traversal of validTraversals) {
    if (traversal.beginGeometryIndex !== expectedIndex || traversal.endGeometryIndex >= trace.length) return null;
    expectedIndex = traversal.endGeometryIndex;
    total += traversal.distanceMetres;
    if (traversal.class === "artwork") artwork += traversal.distanceMetres;
    else connector += traversal.distanceMetres;
    if (traversal.retraced) repeated += traversal.distanceMetres;
  }
  if (expectedIndex !== trace.length - 1) return null;

  if (!isRecord(value.distances) || !keysAre(value.distances, ["artwork_metres", "connector_metres", "total_metres", "repeated_metres", "repeated_included_in_total", "equality_tolerance_metres"])) return null;
  const distances = value.distances;
  if (![distances.artwork_metres, distances.connector_metres, distances.total_metres, distances.repeated_metres].every(nonNegativeInteger)) return null;
  if (distances.repeated_included_in_total !== true || distances.equality_tolerance_metres !== 1 || distances.total_metres === 0) return null;
  const withinTolerance = (left: number, right: unknown) => typeof right === "number" && Math.abs(left - right) <= 1;
  if (!withinTolerance(artwork, distances.artwork_metres) || !withinTolerance(connector, distances.connector_metres) || !withinTolerance(total, distances.total_metres) || !withinTolerance(repeated, distances.repeated_metres) || !withinTolerance(artwork + connector, distances.total_metres) || Number(distances.repeated_metres) > Number(distances.total_metres)) return null;

  if (typeof value.quality_band !== "string" || !qualityBands.includes(value.quality_band as QualityBand)) return null;
  if (!isRecord(value.qualifications) || !keysAre(value.qualifications, ["legality_version", "route_use_advisory_version", "access_verified", "direction_verified", "turns_verified"])) return null;
  if (value.qualifications.legality_version !== "candidate-legality-v1" || value.qualifications.route_use_advisory_version !== "route-use-advisory-v1" || value.qualifications.access_verified !== true || value.qualifications.direction_verified !== true || value.qualifications.turns_verified !== true) return null;

  if (!isRecord(value.versions) || !keysAre(value.versions, ["snapshot", "request", "profile", "template_id", "template", "template_hash", "algorithm", "routing_data", "routing_policy", "candidate"])) return null;
  const versions = value.versions;
  if (versions.snapshot !== "route-candidate-snapshot-v1" || versions.request !== "route-request-v1" || versions.algorithm !== "algorithm-v1" || !["walking-v1", "running-v1", "cycling-v1"].includes(String(versions.profile)) || !positiveInteger(versions.template) || !positiveInteger(versions.candidate) || versions.candidate !== value.candidate_version || typeof versions.template_hash !== "string" || !hash.test(versions.template_hash)) return null;
  for (const key of ["template_id", "routing_data", "routing_policy"] as const) if (typeof versions[key] !== "string" || versions[key].length === 0) return null;

  if (!isRecord(value.provenance) || !keysAre(value.provenance, ["routing_provider"], ["attribution"]) || typeof value.provenance.routing_provider !== "string" || value.provenance.routing_provider.length === 0) return null;
  if (value.provenance.attribution !== undefined && (typeof value.provenance.attribution !== "string" || value.provenance.attribution.length === 0)) return null;

  const start = position(value.start);
  const finish = position(value.finish);
  if (!start || !finish || !samePosition(start, trace[0]!) || !samePosition(finish, trace.at(-1)!)) return null;
  const pace = ({ "walking-v1": 720, "running-v1": 360, "cycling-v1": 240 } as const)[String(versions.profile) as "walking-v1" | "running-v1" | "cycling-v1"];
  const expectedDuration = Math.ceil(Number(distances.total_metres) * pace / 1000);
  if (value.duration_seconds !== expectedDuration) return null;

  if (!Array.isArray(value.instructions) || value.instructions.length < 2 || !Array.isArray(value.direction_markers) || value.direction_markers.length !== validTraversals.length) return null;
  const instructions: RouteInstruction[] = [];
  const directionMarkers: DirectionMarker[] = [];
  let previousInstructionIndex = -1;
  for (let index = 0; index < value.instructions.length; index += 1) {
    const instruction = value.instructions[index];
    if (!isRecord(instruction) || !keysAre(instruction, ["kind", "geometry_index"], ["provider_code"]) || !["depart", "via", "arrive", "provider"].includes(String(instruction.kind)) || !nonNegativeInteger(instruction.geometry_index) || instruction.geometry_index >= trace.length || instruction.geometry_index < previousInstructionIndex) return null;
    const hasProviderCode = nonNegativeInteger(instruction.provider_code) && Number(instruction.provider_code) <= 255;
    if ((instruction.kind === "provider") !== hasProviderCode) return null;
    previousInstructionIndex = instruction.geometry_index;
    instructions.push(Object.freeze({
      kind: instruction.kind as RouteInstruction["kind"],
      geometryIndex: instruction.geometry_index,
      ...(instruction.kind === "provider" ? { providerCode: Number(instruction.provider_code) } : {}),
    }));
  }
  if (instructions[0]?.kind !== "depart" || instructions[0].geometryIndex !== 0 || instructions.at(-1)?.kind !== "arrive" || instructions.at(-1)?.geometryIndex !== trace.length - 1) return null;
  for (let index = 0; index < validTraversals.length; index += 1) {
    const traversal = validTraversals[index]!;
    const marker = value.direction_markers[index];
    const markerPosition = isRecord(marker) ? position(marker.coordinate) : null;
    if (!isRecord(marker) || !keysAre(marker, ["traversal_index", "coordinate", "direction"]) || marker.traversal_index !== index || marker.direction !== traversal.direction || !markerPosition || !samePosition(markerPosition, trace[traversal.beginGeometryIndex]!)) return null;
    directionMarkers.push(Object.freeze({ traversalIndex: index, position: markerPosition, direction: traversal.direction }));
  }

  return Object.freeze({
    id: value.id,
    generationJobId: value.generation_job_id,
    version: value.candidate_version,
    requestRevision: value.request_revision,
    trace: Object.freeze(trace),
    traversals: Object.freeze(validTraversals),
    artworkMetres: Number(distances.artwork_metres),
    connectorMetres: Number(distances.connector_metres),
    totalMetres: Number(distances.total_metres),
    repeatedMetres: Number(distances.repeated_metres),
    repeatedIncludedInTotal: true,
    distanceEqualityToleranceMetres: 1,
    qualityBand: value.quality_band as QualityBand,
    qualifications: Object.freeze({ legalityVersion: "candidate-legality-v1", routeUseAdvisoryVersion: "route-use-advisory-v1", accessVerified: true, directionVerified: true, turnsVerified: true }),
    versions: Object.freeze({ snapshot: "route-candidate-snapshot-v1", request: versions.request as string, profile: versions.profile as string, templateId: versions.template_id as string, template: versions.template as number, templateHash: versions.template_hash, algorithm: versions.algorithm as string, routingData: versions.routing_data as string, routingPolicy: versions.routing_policy as string, candidate: versions.candidate as number }),
    generatedAtEpochSeconds: value.generated_at_epoch_seconds,
    provenance: Object.freeze({ routingProvider: value.provenance.routing_provider, ...(value.provenance.attribution === undefined ? {} : { attribution: value.provenance.attribution as string }) }),
    start,
    finish,
    durationSeconds: expectedDuration,
    instructions: Object.freeze(instructions),
    directionMarkers: Object.freeze(directionMarkers),
  });
}

export function encodeRouteSnapshot(snapshot: RouteSnapshot): unknown {
  const coordinate = (point: Position) => [point.longitude, point.latitude] as const;
  return {
    id: snapshot.id,
    generation_job_id: snapshot.generationJobId,
    candidate_version: snapshot.version,
    request_revision: snapshot.requestRevision,
    geometry: { type: "LineString", coordinates: snapshot.trace.map(coordinate) },
    traversals: snapshot.traversals.map((traversal) => ({
      edge_id: traversal.edgeId,
      class: traversal.class,
      direction: traversal.direction,
      begin_geometry_index: traversal.beginGeometryIndex,
      end_geometry_index: traversal.endGeometryIndex,
      distance_metres: traversal.distanceMetres,
      retraced: traversal.retraced,
    })),
    distances: {
      artwork_metres: snapshot.artworkMetres,
      connector_metres: snapshot.connectorMetres,
      total_metres: snapshot.totalMetres,
      repeated_metres: snapshot.repeatedMetres,
      repeated_included_in_total: snapshot.repeatedIncludedInTotal,
      equality_tolerance_metres: snapshot.distanceEqualityToleranceMetres,
    },
    quality_band: snapshot.qualityBand,
    qualifications: {
      legality_version: snapshot.qualifications.legalityVersion,
      route_use_advisory_version: snapshot.qualifications.routeUseAdvisoryVersion,
      access_verified: snapshot.qualifications.accessVerified,
      direction_verified: snapshot.qualifications.directionVerified,
      turns_verified: snapshot.qualifications.turnsVerified,
    },
    versions: {
      snapshot: snapshot.versions.snapshot,
      request: snapshot.versions.request,
      profile: snapshot.versions.profile,
      template_id: snapshot.versions.templateId,
      template: snapshot.versions.template,
      template_hash: snapshot.versions.templateHash,
      algorithm: snapshot.versions.algorithm,
      routing_data: snapshot.versions.routingData,
      routing_policy: snapshot.versions.routingPolicy,
      candidate: snapshot.versions.candidate,
    },
    generated_at_epoch_seconds: snapshot.generatedAtEpochSeconds,
    provenance: {
      routing_provider: snapshot.provenance.routingProvider,
      ...(snapshot.provenance.attribution === undefined ? {} : { attribution: snapshot.provenance.attribution }),
    },
    start: coordinate(snapshot.start),
    finish: coordinate(snapshot.finish),
    duration_seconds: snapshot.durationSeconds,
    instructions: snapshot.instructions.map((instruction) => ({
      kind: instruction.kind,
      geometry_index: instruction.geometryIndex,
      ...(instruction.providerCode === undefined ? {} : { provider_code: instruction.providerCode }),
    })),
    direction_markers: snapshot.directionMarkers.map((marker) => ({ traversal_index: marker.traversalIndex, coordinate: coordinate(marker.position), direction: marker.direction })),
  };
}
