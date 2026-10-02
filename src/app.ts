import "@fontsource/inter/400.css";
import "@fontsource/inter/500.css";
import "@fontsource/inter/600.css";
import "@fontsource/inter/700.css";
import "@phosphor-icons/web/regular";
import "./styles.css";
import { loadRouteSnapshots } from "./client/storage.ts";
import { unavailableMap } from "./app/map.ts";
import { currentScreen, navigate, type Screen } from "./app/router.ts";
import { renderScreen } from "./app/screen-controller.ts";
import { initialState, updateRequest } from "./app/state.ts";

let state = initialState();
const app = document.querySelector<HTMLElement>("#app");
if (!app) throw new Error("GPSART requires an #app element");
const appRoot = app;

function render(): void {
  const screen = currentScreen(window.location.hash, document.body.dataset.screen);
  const loaded = loadRouteSnapshots(window.localStorage);
  const workspace = screen === "login" || screen === "auth-pending" || screen === "auth-error" || screen === "welcome" || screen === "account" || screen === "screens" ? renderScreen(screen, state, loaded) : `<main id="content" class="workspace"><section class="panel" aria-label="${screen}">${renderScreen(screen, state, loaded)}</section>${unavailableMap()}</main>`;
  appRoot.innerHTML = `<header class="header"><a class="brand" href="#/planner" aria-label="GPSART home">GPSART</a><nav class="nav" aria-label="Main navigation">${["planner", "saved"].map((item) => `<a href="#/${item}" class="${screen === item ? "active" : ""}">${item === "planner" ? "Planner" : "Saved"}</a>`).join("")}</nav><div class="header-end"><a class="signin" href="#/login">Sign in</a></div></header>${workspace}`;
  document.title = `${screen} · GPSART`;
  bind();
}

function bind(): void {
  document.querySelectorAll<HTMLButtonElement>("[data-activity]").forEach((button) => button.addEventListener("click", () => { state = updateRequest(state, { activity: button.dataset.activity as typeof state.request.activity }); render(); }));
  document.querySelectorAll<HTMLButtonElement>("[data-design]").forEach((button) => button.addEventListener("click", () => { state = updateRequest(state, { design: button.dataset.design ?? state.request.design }); navigate("planner"); }));
  const form = document.querySelector<HTMLFormElement>("#plan-form");
  form?.addEventListener("submit", (event) => { event.preventDefault(); const start = document.querySelector<HTMLInputElement>("#start")?.value.trim() ?? ""; const distance = Number(document.querySelector<HTMLInputElement>("#distance")?.value); const returnsToStart = document.querySelector<HTMLInputElement>("#loop")?.checked ?? true; if (!start || !Number.isFinite(distance) || distance < 1 || distance > 100) return; state = updateRequest(state, { start, distanceKm: distance, returnsToStart }); navigate("generating"); });
}

window.addEventListener("hashchange", render);
render();
