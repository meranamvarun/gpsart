import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@phosphor-icons/web/regular";
import "./styles.css";
import { loadRouteSnapshots } from "./client/storage.ts";
import { renderMap } from "./app/map.ts";
import { currentScreen, navigate } from "./app/router.ts";
import { isStandaloneScreen, renderScreen } from "./app/screen-controller.ts";
import { beginRequest, initialState, updateRequest, updateUi } from "./app/state.ts";
import type { DesignCategory } from "./app/catalogue.ts";
import type { RouteRequest } from "./domain/planner.ts";

let state = initialState();
const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("GPSART requires an #app element");
const appRoot = app;

function header(screen: string): string {
  const plannerActive = !["saved", "login", "account", "welcome", "screens"].includes(screen);
  return `<header class="header"><a class="brand" href="#/planner" aria-label="GPSART home">GPSART</a><nav class="nav" aria-label="Main navigation"><a href="#/planner" class="${plannerActive ? "active" : ""}">Planner</a><a href="#/saved" class="${screen === "saved" ? "active" : ""}">Saved</a></nav><div class="header-end"><span class="pilot"><i class="ph ph-leaf" aria-hidden="true"></i> Bengaluru pilot</span><a class="signin" href="#/${state.signedIn ? "account" : "login"}">${state.signedIn ? "My account" : "Sign in"}</a></div></header>`;
}

function render(): void {
  const screen = currentScreen(window.location.hash, document.body.dataset.screen);
  const loaded = loadRouteSnapshots(window.localStorage);
  const body = isStandaloneScreen(screen)
    ? renderScreen(screen, state, loaded)
    : `<main id="content" class="workspace ${state.mapExpanded ? "expanded" : ""}"><section class="panel" aria-label="${screen}"><button class="mobile-expand" type="button" data-action="expand"><i class="ph ph-arrows-out-simple" aria-hidden="true"></i> ${state.mapExpanded ? "Reduce map" : "Expand map"}</button>${renderScreen(screen, state, loaded)}</section>${renderMap(screen, state)}</main>`;
  appRoot.innerHTML = `${header(screen)}${body}`;
  document.title = `${screen} · GPSART`;
  bind();
  window.scrollTo(0, 0);
}

function inputValue(id: string): string {
  return document.querySelector<HTMLInputElement>(`#${id}`)?.value.trim() ?? "";
}

function patchRequest(changes: Omit<Partial<RouteRequest>, "revision">, rerender = true): void {
  state = updateRequest(state, changes);
  if (rerender) render();
}

function useBrowserLocation(): void {
  const status = document.querySelector<HTMLElement>("#location-status");
  if (!status) return;
  if (!navigator.geolocation) {
    status.textContent = "Location is unavailable. Enter a place or coordinates.";
    return;
  }
  status.textContent = "Waiting for location permission…";
  navigator.geolocation.getCurrentPosition(
    ({ coords }) => {
      const start = `${coords.latitude.toFixed(5)}, ${coords.longitude.toFixed(5)}`;
      patchRequest({ start }, false);
      const input = document.querySelector<HTMLInputElement>("#start");
      if (input) input.value = start;
      status.textContent = "Location selected. Coverage still requires server validation.";
    },
    () => { status.textContent = "Location unavailable or permission denied. Enter a place or coordinates."; },
    { timeout: 10_000, maximumAge: 60_000 },
  );
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
  document.querySelector<HTMLInputElement>("#start")?.addEventListener("change", () => patchRequest({ start: inputValue("start") }));
  document.querySelector<HTMLInputElement>("#distance")?.addEventListener("change", (event) => patchRequest({ distanceKm: Number((event.currentTarget as HTMLInputElement).value) }));
  document.querySelector<HTMLInputElement>("#loop")?.addEventListener("change", (event) => patchRequest({ returnsToStart: (event.currentTarget as HTMLInputElement).checked }));
  document.querySelector<HTMLSelectElement>("#tolerance")?.addEventListener("change", (event) => patchRequest({ tolerancePercent: Number((event.currentTarget as HTMLSelectElement).value) }));
  document.querySelector<HTMLFormElement>("#plan-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    const start = inputValue("start");
    const distanceKm = Number(inputValue("distance"));
    if (!start || !Number.isFinite(distanceKm) || distanceKm < 1 || distanceKm > 100) return;
    state = updateRequest(state, { start, distanceKm });
    state = beginRequest(state);
    navigate("generating");
  });
  document.querySelectorAll<HTMLButtonElement>("[data-action]").forEach((button) => button.addEventListener("click", () => {
    if (button.dataset.action === "expand") { state = updateUi(state, { mapExpanded: !state.mapExpanded }); render(); }
    if (button.dataset.action === "location") useBrowserLocation();
    if (button.dataset.action === "google") navigate("auth-pending");
  }));
}

window.addEventListener("hashchange", render);
render();
