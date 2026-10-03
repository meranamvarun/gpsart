declare module "leaflet" {
  export interface LatLng { lat: number; lng: number }
  export interface LeafletMouseEvent { latlng: LatLng }
  export interface Map {
    on(event: "click", handler: (event: LeafletMouseEvent) => void): Map;
    remove(): Map;
  }
  export interface Layer { addTo(map: Map): Layer }
  export function map(element: HTMLElement, options?: Record<string, unknown>): Map;
  export function tileLayer(url: string, options?: Record<string, unknown>): Layer;
  export function circleMarker(point: readonly [number, number], options?: Record<string, unknown>): Layer;
}

declare module "leaflet/dist/leaflet.css";
