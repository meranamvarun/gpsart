import { decodeRouteSnapshot, encodeRouteSnapshot, type RouteSnapshot } from "../domain/route-snapshot.ts";
import type { RouteRequest } from "../domain/planner.ts";

export const SAVED_ROUTE_DATABASE = "gpsart-saved-routes";
export const SAVED_ROUTE_STORE = "routes";
export const SAVED_ROUTE_SCHEMA_VERSION = 1;

export interface SavedRoute {
  readonly id: string;
  readonly name: string;
  readonly savedAtEpochMilliseconds: number;
  readonly storageLabel: "Saved on this device";
  readonly request: RouteRequest;
  readonly snapshot: RouteSnapshot;
}

export type SavedRouteRecordError = "malformed_record" | "unsupported_schema";
export type StorageFailure = "denied" | "quota_exceeded" | "unavailable";
export type SavedRouteList =
  | { readonly kind: "available"; readonly routes: readonly SavedRoute[]; readonly errors: readonly { readonly id: string; readonly kind: SavedRouteRecordError }[] }
  | { readonly kind: "unavailable"; readonly reason: StorageFailure };
export type SavedRouteLookup =
  | { readonly kind: "available"; readonly route: SavedRoute }
  | { readonly kind: "not_found" }
  | { readonly kind: "record_error"; readonly id: string; readonly error: SavedRouteRecordError }
  | { readonly kind: "unavailable"; readonly reason: StorageFailure };
export type SavedRouteWrite =
  | { readonly kind: "saved"; readonly id: string }
  | { readonly kind: "renamed" }
  | { readonly kind: "deleted" }
  | { readonly kind: "not_found" }
  | { readonly kind: "invalid_name" }
  | { readonly kind: "unavailable"; readonly reason: StorageFailure };

interface StoredRoute {
  id: string;
  schema_version: number;
  name: string;
  saved_at_epoch_milliseconds: number;
  request: unknown;
  snapshot: unknown;
}

const objectKeys = (value: object): string[] => Object.keys(value).sort();
const exactKeys = (value: object, keys: readonly string[]): boolean => JSON.stringify(objectKeys(value)) === JSON.stringify([...keys].sort());
const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);

function decodeRequest(value: unknown): RouteRequest | null {
  if (!record(value) || !exactKeys(value, ["start", "activity", "design", "distanceKm", "tolerancePercent", "returnsToStart", "revision"])) return null;
  if (typeof value.start !== "string" || typeof value.design !== "string" || !["Walking", "Running", "Cycling"].includes(String(value.activity))) return null;
  if (typeof value.distanceKm !== "number" || !Number.isFinite(value.distanceKm) || value.distanceKm < 1 || value.distanceKm > 100) return null;
  if (typeof value.tolerancePercent !== "number" || !Number.isFinite(value.tolerancePercent) || value.tolerancePercent < 0 || value.tolerancePercent > 100) return null;
  if (typeof value.returnsToStart !== "boolean" || !Number.isInteger(value.revision) || (value.revision as number) < 0) return null;
  return Object.freeze({
    start: value.start,
    activity: value.activity as RouteRequest["activity"],
    design: value.design,
    distanceKm: value.distanceKm,
    tolerancePercent: value.tolerancePercent,
    returnsToStart: value.returnsToStart,
    revision: value.revision as number,
  });
}

function decodeStored(value: unknown): { route?: SavedRoute; error?: SavedRouteRecordError; id: string } {
  const id = record(value) && typeof value.id === "string" ? value.id : "unknown-record";
  if (!record(value) || value.schema_version !== SAVED_ROUTE_SCHEMA_VERSION) {
    return { id, error: record(value) && typeof value.schema_version === "number" ? "unsupported_schema" : "malformed_record" };
  }
  if (!exactKeys(value, ["id", "schema_version", "name", "saved_at_epoch_milliseconds", "request", "snapshot"])) return { id, error: "malformed_record" };
  const request = decodeRequest(value.request);
  const snapshot = decodeRouteSnapshot(value.snapshot);
  if (!request || !snapshot || snapshot.id !== id || snapshot.requestRevision !== request.revision || typeof value.name !== "string" || !validName(value.name) || typeof value.saved_at_epoch_milliseconds !== "number" || !Number.isSafeInteger(value.saved_at_epoch_milliseconds) || value.saved_at_epoch_milliseconds < 0) {
    return { id, error: "malformed_record" };
  }
  return { id, route: Object.freeze({ id, name: value.name, savedAtEpochMilliseconds: value.saved_at_epoch_milliseconds, storageLabel: "Saved on this device", request, snapshot }) };
}

function validName(name: string): boolean {
  return name.trim() === name && name.length >= 1 && name.length <= 100;
}

function failure(error: unknown): StorageFailure {
  const name = error instanceof DOMException ? error.name : record(error) && typeof error.name === "string" ? error.name : "";
  if (name === "SecurityError" || name === "NotAllowedError") return "denied";
  if (name === "QuotaExceededError") return "quota_exceeded";
  return "unavailable";
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.addEventListener("success", () => resolve(request.result), { once: true });
    request.addEventListener("error", () => reject(request.error ?? new Error("IndexedDB request failed")), { once: true });
  });
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.addEventListener("complete", () => resolve(), { once: true });
    transaction.addEventListener("abort", () => reject(transaction.error ?? new Error("IndexedDB transaction aborted")), { once: true });
    transaction.addEventListener("error", () => reject(transaction.error ?? new Error("IndexedDB transaction failed")), { once: true });
  });
}

export class IndexedDbSavedRouteRepository {
  constructor(
    private readonly factory: IDBFactory = indexedDB,
    private readonly databaseName = SAVED_ROUTE_DATABASE,
  ) {}

  private async database(): Promise<IDBDatabase> {
    const open = this.factory.open(this.databaseName, SAVED_ROUTE_SCHEMA_VERSION);
    open.addEventListener("upgradeneeded", () => {
      if (!open.result.objectStoreNames.contains(SAVED_ROUTE_STORE)) open.result.createObjectStore(SAVED_ROUTE_STORE, { keyPath: "id" });
    });
    return requestResult(open);
  }

  async list(): Promise<SavedRouteList> {
    let database: IDBDatabase | null = null;
    try {
      database = await this.database();
      const transaction = database.transaction(SAVED_ROUTE_STORE, "readonly");
      const values = await requestResult(transaction.objectStore(SAVED_ROUTE_STORE).getAll());
      await transactionDone(transaction);
      const decoded = values.map(decodeStored);
      const routes = decoded.flatMap((item) => item.route ? [item.route] : []).sort((left, right) => right.savedAtEpochMilliseconds - left.savedAtEpochMilliseconds || left.id.localeCompare(right.id));
      const errors = decoded.flatMap((item) => item.error ? [{ id: item.id, kind: item.error }] : []).sort((left, right) => left.id.localeCompare(right.id));
      return { kind: "available", routes: Object.freeze(routes), errors: Object.freeze(errors) };
    } catch (error) {
      return { kind: "unavailable", reason: failure(error) };
    } finally { database?.close(); }
  }

  async get(id: string): Promise<SavedRouteLookup> {
    let database: IDBDatabase | null = null;
    try {
      database = await this.database();
      const transaction = database.transaction(SAVED_ROUTE_STORE, "readonly");
      const value = await requestResult(transaction.objectStore(SAVED_ROUTE_STORE).get(id));
      await transactionDone(transaction);
      if (value === undefined) return { kind: "not_found" };
      const decoded = decodeStored(value);
      return decoded.route ? { kind: "available", route: decoded.route } : { kind: "record_error", id: decoded.id, error: decoded.error! };
    } catch (error) { return { kind: "unavailable", reason: failure(error) }; }
    finally { database?.close(); }
  }

  async save(input: { readonly name: string; readonly request: RouteRequest; readonly snapshot: RouteSnapshot }): Promise<SavedRouteWrite> {
    if (!validName(input.name)) return { kind: "invalid_name" };
    const stored: StoredRoute = {
      id: input.snapshot.id,
      schema_version: SAVED_ROUTE_SCHEMA_VERSION,
      name: input.name,
      saved_at_epoch_milliseconds: Date.now(),
      request: { ...input.request },
      snapshot: encodeRouteSnapshot(input.snapshot),
    };
    if (!decodeStored(stored).route) return { kind: "unavailable", reason: "unavailable" };
    const result = await this.write((store) => { store.put(stored); });
    return result ?? { kind: "saved", id: stored.id };
  }

  async rename(id: string, name: string): Promise<SavedRouteWrite> {
    if (!validName(name)) return { kind: "invalid_name" };
    let database: IDBDatabase | null = null;
    try {
      database = await this.database();
      const transaction = database.transaction(SAVED_ROUTE_STORE, "readwrite");
      const done = transactionDone(transaction);
      const store = transaction.objectStore(SAVED_ROUTE_STORE);
      const existing = await requestResult(store.get(id));
      if (existing === undefined) { transaction.abort(); try { await done; } catch {} return { kind: "not_found" }; }
      const decoded = decodeStored(existing);
      if (!decoded.route) { transaction.abort(); try { await done; } catch {} return { kind: "unavailable", reason: "unavailable" }; }
      store.put({ ...(existing as StoredRoute), name });
      await done;
      return { kind: "renamed" };
    } catch (error) { return { kind: "unavailable", reason: failure(error) }; }
    finally { database?.close(); }
  }

  async delete(id: string): Promise<SavedRouteWrite> {
    let database: IDBDatabase | null = null;
    try {
      database = await this.database();
      const transaction = database.transaction(SAVED_ROUTE_STORE, "readwrite");
      const done = transactionDone(transaction);
      const store = transaction.objectStore(SAVED_ROUTE_STORE);
      const existing = await requestResult(store.getKey(id));
      if (existing === undefined) { transaction.abort(); try { await done; } catch {} return { kind: "not_found" }; }
      store.delete(id);
      await done;
      return { kind: "deleted" };
    } catch (error) { return { kind: "unavailable", reason: failure(error) }; }
    finally { database?.close(); }
  }

  private async write(operation: (store: IDBObjectStore) => void): Promise<{ kind: "unavailable"; reason: StorageFailure } | null> {
    let database: IDBDatabase | null = null;
    try {
      database = await this.database();
      const transaction = database.transaction(SAVED_ROUTE_STORE, "readwrite");
      const done = transactionDone(transaction);
      try { operation(transaction.objectStore(SAVED_ROUTE_STORE)); }
      catch (error) { transaction.abort(); try { await done; } catch {} throw error; }
      await done;
      return null;
    } catch (error) { return { kind: "unavailable", reason: failure(error) }; }
    finally { database?.close(); }
  }
}
