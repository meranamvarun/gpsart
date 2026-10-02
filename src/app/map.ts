import type { Screen } from "./router.ts";
import type { AppState } from "./state.ts";

const previewScreens: readonly Screen[] = ["planner", "catalogue"];

export function renderMap(screen: Screen, state: AppState): string {
  const preview = previewScreens.includes(screen);
  const label = preview
    ? `<div class="map-label"><strong>${state.request.design} design preview</strong><br>Illustrative artwork only</div>`
    : `<div class="map-label"><strong>Route geometry unavailable</strong><br>A validated routing result is required.</div>`;
  return `<section class="map-wrap" aria-label="${preview ? "Illustrative design map" : "Route map unavailable"}"><img class="map-art" src="/assets/map.jpg" alt="Pale illustrative street-map artwork of Bengaluru">${label}<div class="north" aria-label="North"><i class="ph ph-navigation-arrow" aria-hidden="true"></i></div><div class="map-note">${preview ? "Illustrative design preview · not for navigation" : "No route is displayed · routing service not connected"}</div></section>`;
}
