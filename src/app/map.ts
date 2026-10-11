import type { Screen } from "./router.ts";
import type { AppState } from "./state.ts";
import { buildCandidateComparison } from "./route-presentation.ts";

const previewScreens: readonly Screen[] = ["planner", "catalogue"];

interface BaseMapStatus { innerHTML: string }

export function baseMapTileErrorHandler(status: BaseMapStatus | null): () => void {
  let reported = false;
  return () => {
    if (reported || !status) return;
    reported = true;
    status.innerHTML = "<strong>Base map unavailable.</strong> Complete routed geometry remains visible; route facts remain in the comparison and details panel.";
  };
}

export function renderMap(screen: Screen, state: AppState): string {
  if (screen === "planner") {
    return `<section class="map-wrap" aria-label="Starting-point map"><div id="planner-map" class="map" aria-label="Choose a starting point on the map"></div><div class="map-label"><strong>Choose your start</strong><br>Click the map to propose an exact point</div><div class="north" role="img" aria-label="North"><i class="ph ph-navigation-arrow" aria-hidden="true"></i></div><div class="map-note">Map clicks propose a start · the routing service checks access and coverage</div></section>`;
  }
  if ((screen === "results" || screen === "details") && state.candidateLoad === "available" && state.routeResults.length) {
    const request = state.lastAttempt?.status.requestRevision === state.candidateRequestRevision ? state.lastAttempt.request : state.request;
    const comparison = buildCandidateComparison(state.routeResults, request.distanceKm * 1000);
    const selected = comparison.candidates.find(({ snapshot }) => snapshot.id === state.selectedCandidateId) ?? comparison.candidates[0]!;
    return `<section class="map-wrap route-map-wrap" aria-label="North-up comparison of complete routed geometry"><div id="route-map" class="map" aria-label="Selected complete route geometry, north up"></div><div class="map-label"><strong>${screen === "details" ? "Selected route" : "North-up comparison"}</strong><br>${(selected.snapshot.totalMetres / 1000).toFixed(1)} km · routed geometry</div><div class="north" role="img" aria-label="North, map is north up"><i class="ph ph-navigation-arrow" aria-hidden="true"></i></div><div class="route-legend" aria-label="Route map legend"><span><i class="legend-route"></i> Selected routed path</span><span><i class="legend-alt"></i> Other candidates</span><span><i class="legend-connector"></i> Connector or retrace</span><span>Ideal design overlay: not shown</span></div><div id="base-map-status" class="map-note" role="status">Complete routed geometry · north up</div></section>`;
  }
  const preview = previewScreens.includes(screen);
  const label = preview
    ? `<div class="map-label"><strong>${state.request.design} design preview</strong><br>Illustrative artwork only</div>`
    : `<div class="map-label"><strong>Route geometry unavailable</strong><br>A validated routing result is required.</div>`;
  return `<section class="map-wrap" aria-label="${preview ? "Illustrative design map" : "Route map unavailable"}"><img class="map-art" src="/assets/map.jpg" alt="Pale illustrative street-map artwork of Bengaluru">${label}<div class="north" role="img" aria-label="North"><i class="ph ph-navigation-arrow" aria-hidden="true"></i></div><div class="map-note">${preview ? "Illustrative design preview · not for navigation" : "No route is displayed · routing service not connected"}</div></section>`;
}
