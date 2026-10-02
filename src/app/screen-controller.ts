import { distanceBounds } from "../domain/planner.ts";
import type { LoadedSnapshots } from "../client/storage.ts";
import type { AppState } from "./state.ts";
import type { Screen } from "./router.ts";

const escapeHtml = (value: string) => value.replace(/[&<>'"]/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character] ?? character);
const link = (screen: Screen, label: string) => `<a class="textbutton" href="#/${screen}">${label}</a>`;
const top = (title: string, subtitle: string) => `<div class="section-top">${link("planner", "Back to planner")}<h1>${title}</h1><p class="muted small">${subtitle}</p></div>`;

function planner(state: AppState): string {
  const { request } = state;
  const bounds = distanceBounds(request.distanceKm, request.tolerancePercent);
  return `<div class="intro"><h1>Make your miles<br>a little more creative.</h1><p>Choose a design and request a route from the supported service.</p></div><form id="plan-form"><div class="field"><label for="start">Starting point</label><div class="inputwrap"><input id="start" value="${escapeHtml(request.start)}" required placeholder="Place or latitude, longitude"></div><p class="helper">The start is validated by the routing service; it is never silently moved.</p></div><div class="activities" role="group" aria-label="Activity">${(["Walking", "Running", "Cycling"] as const).map((activity) => `<button type="button" class="activity ${request.activity === activity ? "selected" : ""}" data-activity="${activity}" aria-pressed="${request.activity === activity}">${activity}</button>`).join("")}</div><div class="field"><label for="distance">Target distance</label><div class="inputwrap"><input id="distance" type="number" min="1" max="100" step="0.1" value="${request.distanceKm}" required><span>km</span></div><p class="helper">${bounds.minimumKm.toFixed(1)}–${bounds.maximumKm.toFixed(1)} km at ±${request.tolerancePercent}%</p></div><div class="settings"><label class="between small"><strong>Return to start</strong><span class="toggle"><input id="loop" type="checkbox" ${request.returnsToStart ? "checked" : ""}></span></label></div><button class="primary full" type="submit">Generate routes</button></form>`;
}

function unavailableResult(title: string, copy: string): string {
  return `${top(title, copy)}<div class="notice">No route candidate is shown until the server returns a validated, immutable route snapshot. GPSART does not create geometry, distance, safety, or success data in the browser.</div>${link("planner", "Edit request")}`;
}

function saved(loaded: LoadedSnapshots): string {
  const content = loaded.kind === "unsupported_snapshot" ? "A saved route from an older or unsupported format was found. It has not been loaded." : loaded.kind === "unavailable" ? "Browser storage is unavailable. No route data has been changed." : loaded.snapshots.length ? `${loaded.snapshots.length} validated route snapshot(s) are stored on this device.` : "No validated route snapshots are stored on this device.";
  return `${top("Your next adventures.", "Saved routes stay on this device unless you choose otherwise.")}<div class="notice">${content}</div>${link("planner", "Plan a route")}`;
}

export function renderScreen(screen: Screen, state: AppState, loaded: LoadedSnapshots): string {
  if (screen === "planner") return planner(state);
  if (screen === "catalogue") return `${top("What will you draw?", "Choose a template; local fit is checked by the routing service.")}<div class="designs"><button class="design selected" data-design="Heart">Heart</button><button class="design" data-design="HI">HI</button></div>${link("planner", "Use selected design")}`;
  if (screen === "generating") return `${top("Finding your next adventure.", "A route request is waiting for a routing service.")}<div class="notice">Generation status is unavailable because no routing service is connected. Your request is retained locally in this page only.</div>${link("planner", "Cancel request")}`;
  if (screen === "results") return unavailableResult("Route results", "Validated route candidates will appear here.");
  if (screen === "details") return unavailableResult("Route details", "A validated candidate is required before details can be shown.");
  if (screen === "saved") return saved(loaded);
  if (screen === "no-match") return `${top("We couldn’t find a fit this time.", "No qualifying route was returned within the search budget.")}<div class="notice">Try changing the design or distance. Suggestions are not treated as routing facts.</div>${link("planner", "Edit request")}`;
  if (screen === "coverage") return `${top("Outside our pilot area", "Coverage must be confirmed by the service.")}<div class="notice">No coverage data is assumed in the browser.</div>${link("planner", "Change starting point")}`;
  if (screen === "error") return `${top("The route service is unavailable.", "Your input was not changed.")}<div class="notice">Try again when the service is available.</div>${link("planner", "Back to planner")}`;
  if (screen === "login") return `${top("Your next adventure starts here.", "You can plan as a guest.")}<div class="notice">Authentication is not connected in this build.</div>${link("planner", "Continue as guest")}`;
  if (screen === "auth-pending") return `${top("Connecting account", "Authentication state is pending.")}<div class="notice">No account data is accessed by this prototype.</div>${link("login", "Cancel sign-in")}`;
  if (screen === "auth-error") return `${top("Let’s try that again.", "Authentication could not be completed.")}${link("login", "Return to sign in")}`;
  if (screen === "welcome") return `${top("Welcome to GPSART.", "You’re ready to plan.")}${link("planner", "Start planning")}`;
  if (screen === "account") return `${top("Your account", "Account data is not connected in this build.")}${link("planner", "Back to planner")}`;
  return `<main id="content" class="screen-index"><span class="eyebrow">GPSART · Design review</span><h1>Every step of the adventure.</h1><div class="screen-list">${(["planner", "catalogue", "generating", "results", "details", "saved", "no-match", "coverage", "error", "login", "auth-pending", "auth-error", "welcome", "account"] as Screen[]).map((item) => `<a class="screen-link" href="#/${item}"><h3>${item}</h3></a>`).join("")}</div></main>`;
}
