type Activity = "walking" | "running" | "cycling";
export type ProfileVersion = "walking-v1" | "running-v1" | "cycling-v1";

export type GeoPoint = Readonly<{ longitude: number; latitude: number }>;

export type StartResolution = Readonly<{
  id: string;
  requested_point: GeoPoint;
  candidate_network_point: GeoPoint;
  snap_distance_metres: number;
  profile_version: ProfileVersion;
  coverage_policy_version: string;
  data_version: string;
  expires_at_epoch_seconds: number;
  confirmation_required: true;
}>;

export type StartOutcomeCode =
  | "invalid_coordinates"
  | "unsupported_coverage"
  | "no_routable_access"
  | "provider_failure"
  | "expired_resolution"
  | "wrong_session"
  | "profile_changed"
  | "coverage_policy_changed"
  | "data_version_changed"
  | "snap_too_far"
  | "authorization_required"
  | "invalid_request";

export type CoverageResponse = Readonly<{
  version: string;
  geojson: Readonly<Record<string, unknown>>;
  local_route_feasibility: "not_evaluated";
}>;

export function decodeCoverageResponse(value: unknown): CoverageResponse | undefined {
  if (!value || typeof value !== "object") return undefined;
  const candidate = value as Record<string, unknown>;
  if (
    typeof candidate.version !== "string" || !candidate.version ||
    !candidate.geojson || typeof candidate.geojson !== "object" || Array.isArray(candidate.geojson) ||
    candidate.local_route_feasibility !== "not_evaluated"
  ) return undefined;
  return candidate as CoverageResponse;
}

const outcomeCodes: readonly StartOutcomeCode[] = [
  "invalid_coordinates", "unsupported_coverage", "no_routable_access",
  "provider_failure", "expired_resolution", "wrong_session", "profile_changed",
  "coverage_policy_changed", "data_version_changed", "snap_too_far",
  "authorization_required",
  "invalid_request",
];

export function decodeStartOutcome(value: unknown): StartOutcomeCode | undefined {
  if (!value || typeof value !== "object") return undefined;
  const outcome = (value as Record<string, unknown>).outcome;
  if (!outcome || typeof outcome !== "object") return undefined;
  const code = (outcome as Record<string, unknown>).code;
  return outcomeCodes.includes(code as StartOutcomeCode) ? code as StartOutcomeCode : undefined;
}

export function decodeConfirmedStart(value: unknown): GeoPoint | undefined {
  if (!value || typeof value !== "object") return undefined;
  return point((value as Record<string, unknown>).confirmed_start);
}

const versions: Readonly<Record<Activity, ProfileVersion>> = {
  walking: "walking-v1",
  running: "running-v1",
  cycling: "cycling-v1",
};

function point(value: unknown): GeoPoint | undefined {
  if (!value || typeof value !== "object") return undefined;
  const candidate = value as Record<string, unknown>;
  if (
    typeof candidate.longitude !== "number" || !Number.isFinite(candidate.longitude) ||
    typeof candidate.latitude !== "number" || !Number.isFinite(candidate.latitude) ||
    candidate.longitude < -180 || candidate.longitude > 180 ||
    candidate.latitude < -90 || candidate.latitude > 90
  ) return undefined;
  return { longitude: candidate.longitude, latitude: candidate.latitude };
}

export function decodeStartResolution(value: unknown): StartResolution | undefined {
  if (!value || typeof value !== "object") return undefined;
  const candidate = value as Record<string, unknown>;
  const requested = point(candidate.requested_point);
  const network = point(candidate.candidate_network_point);
  const profileVersion = candidate.profile_version;
  if (
    typeof candidate.id !== "string" || !requested || !network ||
    !Object.values(versions).includes(profileVersion as ProfileVersion) ||
    !Number.isInteger(candidate.snap_distance_metres) || (candidate.snap_distance_metres as number) < 0 ||
    typeof candidate.coverage_policy_version !== "string" ||
    typeof candidate.data_version !== "string" ||
    !Number.isInteger(candidate.expires_at_epoch_seconds) || (candidate.expires_at_epoch_seconds as number) <= 0 ||
    candidate.confirmation_required !== true
  ) return undefined;
  return candidate as StartResolution;
}
