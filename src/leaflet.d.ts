declare module "leaflet" {
  export interface LatLng { lat: number; lng: number }
  export interface LeafletMouseEvent { latlng: LatLng }
  export interface Map {
    on(event: "click", handler: (event: LeafletMouseEvent) => void): Map;
    fitBounds(bounds: LatLngBounds, options?: Record<string, unknown>): Map;
    remove(): Map;
  }
  export type LatLngTuple = readonly [number, number];
  export interface LatLngBounds {}
  export interface Layer {
    addTo(map: Map): Layer;
    bindTooltip(content: string): Layer;
    on(event: "tileerror", handler: () => void): Layer;
  }
  export function map(element: HTMLElement, options?: Record<string, unknown>): Map;
  export function tileLayer(url: string, options?: Record<string, unknown>): Layer;
  export function circleMarker(point: readonly [number, number], options?: Record<string, unknown>): Layer;
  export function polyline(points: readonly LatLngTuple[], options?: Record<string, unknown>): Layer;
  export function latLngBounds(points: readonly LatLngTuple[]): LatLngBounds;
}

declare module "leaflet/dist/leaflet.css";
