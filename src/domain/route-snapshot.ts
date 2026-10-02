export interface Position { readonly latitude: number; readonly longitude: number; }

export interface RouteSnapshot {
  readonly id: string;
  readonly version: number;
  readonly requestRevision: number;
  readonly trace: readonly Position[];
  readonly totalMetres: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function position(value: unknown): Position | null {
  if (!isRecord(value) || typeof value.latitude !== "number" || typeof value.longitude !== "number") return null;
  if (!Number.isFinite(value.latitude) || !Number.isFinite(value.longitude) || value.latitude < -90 || value.latitude > 90 || value.longitude < -180 || value.longitude > 180) return null;
  return { latitude: value.latitude, longitude: value.longitude };
}

export function decodeRouteSnapshot(value: unknown): RouteSnapshot | null {
  if (!isRecord(value)) return null;
  const { id, version, requestRevision, totalMetres, trace: rawTrace } = value;
  if (typeof id !== "string" || id.length === 0 || typeof version !== "number" || !Number.isInteger(version) || version < 1 || typeof requestRevision !== "number" || !Number.isInteger(requestRevision) || requestRevision < 0 || typeof totalMetres !== "number" || !Number.isFinite(totalMetres) || totalMetres < 0 || !Array.isArray(rawTrace)) return null;
  const trace = rawTrace.map(position);
  if (trace.some((item): item is null => item === null)) return null;
  return { id, version, requestRevision, trace: trace as Position[], totalMetres };
}
