import { decodeRouteSnapshot, type RouteSnapshot } from "../domain/route-snapshot.ts";

export const ROUTE_SNAPSHOT_STORAGE_KEY = "gpsart.route-snapshots.v1";

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export type LoadedSnapshots =
  | { readonly kind: "available"; readonly snapshots: readonly RouteSnapshot[] }
  | { readonly kind: "unsupported_snapshot" }
  | { readonly kind: "unavailable" };

export function loadRouteSnapshots(storage: StorageLike): LoadedSnapshots {
  let raw: string | null;
  try { raw = storage.getItem(ROUTE_SNAPSHOT_STORAGE_KEY); } catch { return { kind: "unavailable" }; }
  if (raw === null) return { kind: "available", snapshots: [] };
  try {
    const decoded: unknown = JSON.parse(raw);
    if (!Array.isArray(decoded)) return { kind: "unsupported_snapshot" };
    const snapshots = decoded.map(decodeRouteSnapshot);
    if (snapshots.some((snapshot): snapshot is null => snapshot === null)) return { kind: "unsupported_snapshot" };
    return { kind: "available", snapshots: snapshots as RouteSnapshot[] };
  } catch { return { kind: "unsupported_snapshot" }; }
}

export function saveRouteSnapshots(storage: StorageLike, snapshots: readonly RouteSnapshot[]): boolean {
  try { storage.setItem(ROUTE_SNAPSHOT_STORAGE_KEY, JSON.stringify(snapshots)); return true; } catch { return false; }
}
