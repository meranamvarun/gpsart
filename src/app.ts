import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@phosphor-icons/web/regular";
import "leaflet/dist/leaflet.css";
import * as L from "leaflet";
import "./styles.css";
import { readCsrfCookie } from "./client/guest-session.ts";
import { fetchRouteCandidates } from "./client/api.ts";
import {
  PlannerWorkflow,
  type PlannerWorkflowSnapshot,
} from "./client/planner-workflow.ts";
import { loadRouteSnapshots } from "./client/storage.ts";
import { baseMapTileErrorHandler, renderMap } from "./app/map.ts";
import { currentScreen, navigate, type Screen } from "./app/router.ts";
import { screenForWorkflow } from "./app/outcome-flow.ts";
import { isStandaloneScreen, renderScreen } from "./app/screen-controller.ts";
import { acceptCandidateLoad, beginCandidateLoad, initialState, recordCompletedAttempt, retryCompletedAttempt, selectCandidate, updateRequest, updateUi } from "./app/state.ts";
import { buildCandidateComparison, routeMapModel } from "./app/route-presentation.ts";
import type { DesignCategory } from "./app/catalogue.ts";
import type { RouteRequest } from "./domain/planner.ts";
import {
  fromMapSelection,
  fromSearchSelection,
  parseManualCoordinates,
  requestBrowserStart,
  type ProposedStart,
} from "./domain/start-selection.ts";
import type { GeoPoint } from "./domain/start-resolution.ts";

let state = initialState();
let proposedStart: ProposedStart | null = null;
let workflowSnapshot: PlannerWorkflowSnapshot | null = null;
let locationMessage = "Location is requested only when you choose. You can always enter coordinates or click the map.";
let leafletMap: L.Map | null = null;
let resuming = false;

const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("GPSART requires an #app element");
const appRoot = app;

const workflow = new PlannerWorkflow({
  csrfToken: readCsrfCookie,
  currentRevision: () => state.request.revision,
  onChange: handleWorkflowChange,
});

function header(screen: string): string {
  const plannerActive = !["saved", "login", "account", "welcome", "screens"].includes(screen);
  return `<header class="header"><a class="brand" href="#/planner" aria-label="GPSART home">GPSART</a><nav class="nav" aria-label="Main navigation"><a href="#/planner" class="${plannerActive ? "active" : ""}">Planner</a><a href="#/saved" class="${screen === "saved" ? "active" : ""}">Saved</a></nav><div class="header-end"><span class="pilot"><i class="ph ph-leaf" aria-hidden="true"></i> Bengaluru pilot</span><a class="signin" href="#/${state.signedIn ? "account" : "login"}">${state.signedIn ? "My account" : "Sign in"}</a></div></header>`;
}

function render(): void {
  leafletMap?.remove();
  leafletMap = null;
  const screen = currentScreen(window.location.hash, document.body.dataset.screen);
  const loaded = loadRouteSnapshots(window.localStorage);
  const body = isStandaloneScreen(screen)
    ? renderScreen(screen, state, loaded, workflowSnapshot)
    : `<main id="content" class="workspace ${state.mapExpanded ? "expanded" : ""}"><section class="panel" aria-label="${screen}"><button class="mobile-expand" type="button" data-action="expand"><i class="ph ph-arrows-out-simple" aria-hidden="true"></i> ${state.mapExpanded ? "Reduce map" : "Expand map"}</button>${renderScreen(screen, state, loaded, workflowSnapshot)}</section>${renderMap(screen, state)}</main>`;
  appRoot.innerHTML = `${header(screen)}${body}`;
  document.title = `${screen} · GPSART`;
  bind();
  bindPlannerMap(screen);
  bindRouteMap(screen);
  const locationStatus = document.querySelector<HTMLElement>("#location-status");
  if (locationStatus) locationStatus.textContent = locationMessage;
  window.scrollTo(0, 0);
}

function inputValue(id: string): string {
  return document.querySelector<HTMLInputElement>(`#${id}`)?.value.trim() ?? "";
}

function patchRequest(changes: Omit<Partial<RouteRequest>, "revision">, rerender = true): void {
  state = updateRequest(state, changes);
  if (rerender) render();
}

function selectStart(start: ProposedStart): void {
  proposedStart = start;
  locationMessage = start.source === "geolocation"
    ? "Location selected. Coverage and road access still require server validation."
    : "Start selected. Coverage and road access still require server validation.";
  patchRequest({ start: start.label });
}

async function useBrowserLocation(): Promise<void> {
  const status = document.querySelector<HTMLElement>("#location-status");
  if (status) status.textContent = "Waiting for location permission…";
  const result = await requestBrowserStart();
  if (result.kind === "selected") {
    selectStart(result.start);
    return;
  }
  locationMessage = result.kind === "denied"
    ? "Location permission was denied. Enter coordinates or click the map instead."
    : result.kind === "timeout"
      ? "Location timed out. Enter coordinates or click the map instead."
      : result.kind === "unsupported"
        ? "This browser cannot provide location. Enter coordinates or click the map instead."
        : result.kind === "low_accuracy"
          ? `Location accuracy (${Math.round(result.accuracyMetres)} m) is too low. Enter coordinates or click the map instead.`
          : "Location is unavailable. Enter coordinates or click the map instead.";
  if (status) status.textContent = locationMessage;
}

function bindPlannerMap(screen: Screen): void {
  if (screen !== "planner") return;
  const element = document.querySelector<HTMLElement>("#planner-map");
  if (!element) return;
  const center: readonly [number, number] = proposedStart
    ? [proposedStart.point.latitude, proposedStart.point.longitude]
    : [12.9716, 77.5946];
  leafletMap = L.map(element, { center, zoom: proposedStart ? 15 : 12, zoomControl: true });
  L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
    attribution: "&copy; OpenStreetMap contributors",
    maxZoom: 19,
  }).addTo(leafletMap);
  if (proposedStart) {
    L.circleMarker(center, { radius: 8, color: "#154c36", fillColor: "#faf8f1", fillOpacity: 1, weight: 3 }).addTo(leafletMap);
  }
  leafletMap.on("click", ({ latlng }) => {
    const selected = fromMapSelection({ latitude: latlng.lat, longitude: latlng.lng });
    if (selected) selectStart(selected);
  });
}

function bindRouteMap(screen: Screen): void {
  if (screen !== "results" && screen !== "details") return;
  const element = document.querySelector<HTMLElement>("#route-map");
  if (!element || state.candidateLoad !== "available" || !state.routeResults.length) return;
  const request = state.lastAttempt?.status.requestRevision === state.candidateRequestRevision ? state.lastAttempt.request : state.request;
  const comparison = buildCandidateComparison(state.routeResults, request.distanceKm * 1000);
  const model = routeMapModel(comparison, state.selectedCandidateId ?? "");
  const center = model.highlighted.trace[0]!;
  leafletMap = L.map(element, { center: [center.latitude, center.longitude], zoom: 15, zoomControl: true });
  const status = document.querySelector<HTMLElement>("#base-map-status");
  L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
    attribution: "&copy; OpenStreetMap contributors",
    maxZoom: 19,
  }).on("tileerror", baseMapTileErrorHandler(status)).addTo(leafletMap);
  for (const alternative of model.alternatives) {
    L.polyline(alternative.trace.map((point) => [point.latitude, point.longitude]), { color: "#7d837e", opacity: 0.5, weight: 4 }).addTo(leafletMap);
  }
  const selected = state.routeResults.find(({ id }) => id === model.highlighted.id)!;
  for (const traversal of selected.traversals) {
    const points = selected.trace.slice(traversal.beginGeometryIndex, traversal.endGeometryIndex + 1).map((point) => [point.latitude, point.longitude] as L.LatLngTuple);
    const connector = traversal.class !== "artwork" || traversal.retraced;
    L.polyline(points, { color: connector ? "#f58a23" : "#154c36", opacity: 1, weight: 6, dashArray: connector ? "9 7" : undefined }).addTo(leafletMap);
  }
  for (const marker of model.highlighted.directionMarkers) {
    L.circleMarker([marker.position.latitude, marker.position.longitude], { radius: 4, color: "#154c36", fillColor: "#faf8f1", fillOpacity: 1, weight: 2 }).bindTooltip(marker.direction).addTo(leafletMap);
  }
  L.circleMarker([selected.start.latitude, selected.start.longitude], { radius: 7, color: "#154c36", fillColor: "#faf8f1", fillOpacity: 1, weight: 3 }).bindTooltip("Start").addTo(leafletMap);
  L.circleMarker([selected.finish.latitude, selected.finish.longitude], { radius: 7, color: "#8c3f2f", fillColor: "#faf8f1", fillOpacity: 1, weight: 3 }).bindTooltip("Finish").addTo(leafletMap);
  const bounds = L.latLngBounds(model.highlighted.trace.map((point) => [point.latitude, point.longitude] as L.LatLngTuple));
  leafletMap.fitBounds(bounds, { padding: [35, 35], maxZoom: 17 });
}

function showScreen(screen: Screen): void {
  if (currentScreen(window.location.hash, document.body.dataset.screen) === screen) render();
  else navigate(screen);
}

function handleWorkflowChange(snapshot: PlannerWorkflowSnapshot): void {
  workflowSnapshot = snapshot;
  if (resuming && snapshot.request) {
    state = { ...state, request: snapshot.request, outcome: "pending", routeResult: null };
    proposedStart = snapshot.proposedStart;
  }
  const completed = snapshot.phase === "terminal" ? snapshot.status
    : snapshot.phase === "stale" ? snapshot.previousStatus
      : null;
  if (snapshot.request && completed) {
    state = recordCompletedAttempt(state, snapshot.request, completed);
  }
  if (snapshot.request && completed?.outcome?.code === "succeeded" && completed.resultIds.length && state.candidateRequestRevision !== snapshot.request.revision) {
    const requestRevision = snapshot.request.revision;
    const loadingState = beginCandidateLoad(state, requestRevision);
    if (loadingState !== state) {
      state = loadingState;
      void fetchRouteCandidates(completed.resultIds).then((result) => {
        state = acceptCandidateLoad(state, requestRevision, result);
        showScreen("results");
      });
    }
  }
  showScreen(screenForWorkflow(snapshot));
}

function submitPlan(): void {
  const start = inputValue("start");
  const distanceKm = Number(inputValue("distance"));
  const selected = proposedStart?.label === start ? proposedStart : parseManualCoordinates(start);
  if (!selected) {
    locationMessage = "Enter exact latitude, longitude coordinates, click the map, use location, or choose a search result.";
    const status = document.querySelector<HTMLElement>("#location-status");
    if (status) {
      status.textContent = locationMessage;
      status.classList.add("error");
      status.setAttribute("role", "alert");
    }
    return;
  }
  proposedStart = selected;
  if (state.request.start !== start || state.request.distanceKm !== distanceKm) {
    state = updateRequest(state, { start, distanceKm });
  }
  void workflow.submit(state.request, selected);
}

function retryDependency(): void {
  const retried = retryCompletedAttempt(state);
  if (!retried) return;
  const selected = proposedStart?.label === retried.request.start
    ? proposedStart
    : parseManualCoordinates(retried.request.start);
  if (!selected) {
    locationMessage = "Re-select the original start before retrying this request.";
    showScreen("planner");
    return;
  }
  state = retried;
  proposedStart = selected;
  void workflow.submit(state.request, selected);
}

function bind(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-activity]").forEach((button) => button.addEventListener("click", () => {
    const activity = button.dataset.activity as RouteRequest["activity"] | undefined;
    if (activity) patchRequest({ activity });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-category]").forEach((button) => button.addEventListener("click", () => {
    const category = button.dataset.category as DesignCategory | undefined;
    if (category) { state = updateUi(state, { category }); render(); }
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-design]").forEach((button) => button.addEventListener("click", () => {
    const design = button.dataset.design;
    if (design) patchRequest({ design });
  }));
  document.querySelectorAll<HTMLButtonElement>("[data-candidate-id]").forEach((button) => button.addEventListener("click", () => {
    const candidateId = button.dataset.candidateId;
    if (candidateId) { state = selectCandidate(state, candidateId); render(); }
  }));
  document.querySelector<HTMLInputElement>("#start")?.addEventListener("change", () => {
    const start = inputValue("start");
    proposedStart = parseManualCoordinates(start) ?? null;
    patchRequest({ start });
  });
  document.querySelector<HTMLInputElement>("#distance")?.addEventListener("change", (event) => patchRequest({ distanceKm: Number((event.currentTarget as HTMLInputElement).value) }));
  document.querySelector<HTMLInputElement>("#loop")?.addEventListener("change", (event) => patchRequest({ returnsToStart: (event.currentTarget as HTMLInputElement).checked }));
  document.querySelector<HTMLSelectElement>("#tolerance")?.addEventListener("change", (event) => patchRequest({ tolerancePercent: Number((event.currentTarget as HTMLSelectElement).value) }));
  document.querySelector<HTMLFormElement>("#plan-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    submitPlan();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action]").forEach((button) => button.addEventListener("click", () => {
    if (button.dataset.action === "expand") { state = updateUi(state, { mapExpanded: !state.mapExpanded }); render(); }
    if (button.dataset.action === "location") void useBrowserLocation();
    if (button.dataset.action === "confirm-start") void workflow.confirmAndStart();
    if (button.dataset.action === "cancel-job") void workflow.cancel();
    if (button.dataset.action === "retry-job") retryDependency();
    if (button.dataset.action === "edit-start") document.querySelector<HTMLInputElement>("#start")?.select();
    if (button.dataset.action === "google") navigate("auth-pending");
  }));
}

window.addEventListener("gpsart:search-selection", (event) => {
  const detail = (event as CustomEvent<{ point?: GeoPoint; label?: string }>).detail;
  if (!detail?.point || typeof detail.label !== "string") return;
  const selected = fromSearchSelection(detail.point, detail.label);
  if (selected) selectStart(selected);
});
window.addEventListener("gpsart:session-ready", () => {
  resuming = true;
  void workflow.resume().finally(() => { resuming = false; });
});
window.addEventListener("hashchange", render);
window.addEventListener("pagehide", () => workflow.dispose());
render();
