import type { GeoPoint } from "./start-resolution.ts";

export type StartSource = "manual" | "search" | "map" | "geolocation";

export type ProposedStart = Readonly<{
  point: GeoPoint;
  source: StartSource;
  label: string;
  accuracyMetres?: number;
}>;

export type BrowserStartResult =
  | { readonly kind: "selected"; readonly start: ProposedStart }
  | { readonly kind: "unsupported" | "denied" | "timeout" | "unavailable" }
  | { readonly kind: "low_accuracy"; readonly accuracyMetres: number };

export const GEOLOCATION_MAX_ACCURACY_METRES = 100;

function validPoint(point: GeoPoint): boolean {
  return Number.isFinite(point.latitude) && Number.isFinite(point.longitude)
    && point.latitude >= -90 && point.latitude <= 90
    && point.longitude >= -180 && point.longitude <= 180;
}

function coordinateLabel(point: GeoPoint): string {
  return `${point.latitude.toFixed(5)}, ${point.longitude.toFixed(5)}`;
}

function proposed(point: GeoPoint, source: StartSource, label: string, accuracyMetres?: number): ProposedStart | undefined {
  if (!validPoint(point) || !label.trim()) return undefined;
  return Object.freeze({ point: Object.freeze({ ...point }), source, label: label.trim(), ...(accuracyMetres === undefined ? {} : { accuracyMetres }) });
}

export function parseManualCoordinates(value: string): ProposedStart | undefined {
  const match = value.trim().match(/^([+-]?(?:\d+(?:\.\d+)?|\.\d+))\s*,\s*([+-]?(?:\d+(?:\.\d+)?|\.\d+))$/);
  if (!match) return undefined;
  const point = { latitude: Number(match[1]), longitude: Number(match[2]) };
  return proposed(point, "manual", value.trim());
}

export function fromSearchSelection(point: GeoPoint, label: string): ProposedStart | undefined {
  return proposed(point, "search", label);
}

export function fromMapSelection(point: GeoPoint): ProposedStart | undefined {
  return proposed(point, "map", coordinateLabel(point));
}

export function requestBrowserStart(
  geolocation: Geolocation | undefined = globalThis.navigator?.geolocation,
): Promise<BrowserStartResult> {
  if (!geolocation) return Promise.resolve({ kind: "unsupported" });
  return new Promise((resolve) => {
    geolocation.getCurrentPosition(
      ({ coords }) => {
        if (!Number.isFinite(coords.accuracy) || coords.accuracy > GEOLOCATION_MAX_ACCURACY_METRES) {
          resolve({ kind: "low_accuracy", accuracyMetres: coords.accuracy });
          return;
        }
        const point = { latitude: coords.latitude, longitude: coords.longitude };
        const start = proposed(point, "geolocation", coordinateLabel(point), coords.accuracy);
        resolve(start ? { kind: "selected", start } : { kind: "unavailable" });
      },
      (error) => {
        if (error.code === 1) resolve({ kind: "denied" });
        else if (error.code === 3) resolve({ kind: "timeout" });
        else resolve({ kind: "unavailable" });
      },
      { enableHighAccuracy: true, timeout: 10_000, maximumAge: 60_000 },
    );
  });
}
