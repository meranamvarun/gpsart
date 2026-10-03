import { distanceBounds } from "../domain/planner.ts";
import type { LoadedSnapshots } from "../client/storage.ts";
import { previewsFor } from "./catalogue.ts";
import type { AppState } from "./state.ts";
import { screens, type Screen } from "./router.ts";
import type { PlannerWorkflowSnapshot, WorkflowIssue } from "../client/planner-workflow.ts";

const escapeHtml = (value: string): string => value.replace(/[&<>'"]/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character] ?? character);
const icon = (name: string): string => `<i class="ph ph-${name}" aria-hidden="true"></i>`;
const link = (screen: Screen, label: string): string => `<a class="textbutton" href="#/${screen}">${label}</a>`;
const top = (title: string, subtitle: string, back: Screen = "planner"): string => `<div class="section-top">${link(back, `${icon("arrow-left")} ${back === "results" ? "Back to results" : "Back to planner"}`)}<h1>${title}</h1><p class="muted small">${subtitle}</p></div>`;

function summary(state: AppState): string {
  const request = state.request;
  return `<div class="summary"><strong>${escapeHtml(request.design)} · ${request.activity} · ${request.distanceKm.toFixed(1)} km</strong>${escapeHtml(request.start)}<br>±${request.tolerancePercent}% · ${request.returnsToStart ? "Return to start" : "Separate finish · travel home excluded"}</div>`;
}

function designPicker(state: AppState, full = false): string {
  const previews = previewsFor(state.category);
  const shown = full ? previews : previews.slice(0, 4);
  return `<div class="tabs" role="tablist" aria-label="Design category">${(["Shapes", "Text"] as const).map((category) => `<button type="button" role="tab" aria-selected="${state.category === category}" class="${state.category === category ? "active" : ""}" data-category="${category}">${category}</button>`).join("")}</div><div class="designs">${shown.map(({ name, icon: iconName, complexity }) => `<button type="button" class="design ${name === state.request.design ? "selected" : ""}" data-design="${name}" aria-label="${name}, ${complexity}" aria-pressed="${name === state.request.design}">${iconName ? icon(iconName) : `<b>${name}</b>`}${full ? `<span class="design-name">${name} · ${complexity}</span>` : ""}</button>`).join("")}</div>`;
}

function workflowIssue(issue: WorkflowIssue | null): string {
  if (!issue) return "";
  const messages: Partial<Record<WorkflowIssue, string>> = {
    invalid_fields: "Choose a start using exact coordinates, the map, location, or a search result, then check every field.",
    active_job: "Your earlier request is still active. Cancel it or let it finish before starting another.",
    unsupported_coverage: "That start is outside currently verified coverage. Your plan has not been submitted.",
    no_routable_access: "The service could not verify routable access near that point.",
    snap_too_far: "The nearest routable point is too far away. Choose a different start.",
    network_unavailable: "The route service could not be reached. Your inputs are unchanged.",
    csrf_unavailable: "Your private guest session is not ready. Please try again.",
    expired_job: "That route request expired. Your inputs remain available for a new request.",
  };
  return `<p class="error" role="alert">${messages[issue] ?? "The service could not accept this request. Your inputs are unchanged."}</p>`;
}

function plannerRuntime(workflow: PlannerWorkflowSnapshot | null, currentRevision: number): string {
  if (!workflow) return "";
  if (workflow.phase === "awaiting_confirmation" && workflow.resolution && workflow.request?.revision === currentRevision) {
    const candidate = workflow.resolution.candidate_network_point;
    return `<section class="confirmation" aria-labelledby="confirmation-title"><span class="eyebrow">Confirmation required</span><h3 id="confirmation-title">Use the verified road-access point?</h3><p class="small muted">The service found a point ${workflow.resolution.snap_distance_metres} m from your request at ${candidate.latitude.toFixed(5)}, ${candidate.longitude.toFixed(5)}. No route job starts until you confirm.</p><div class="actions"><button type="button" class="secondary" data-action="edit-start">Edit start</button><button type="button" class="primary" data-action="confirm-start">Confirm and generate</button></div></section>`;
  }
  if (workflow.phase === "stale") return `<div class="notice" role="status">A previous request finished, but it cannot replace your newer plan.</div>`;
  return "";
}

function planner(state: AppState, workflow: PlannerWorkflowSnapshot | null): string {
  const request = state.request;
  const validDistance = Number.isFinite(request.distanceKm) && request.distanceKm >= 1 && request.distanceKm <= 100;
  const validTolerance = Number.isFinite(request.tolerancePercent) && request.tolerancePercent >= 0 && request.tolerancePercent <= 100;
  const bounds = distanceBounds(validDistance ? request.distanceKm : 1, validTolerance ? request.tolerancePercent : 0);
  const busy = workflow?.phase === "resolving" || workflow?.phase === "submitting";
  return `<div class="intro"><h1>Make your miles<br>a little more creative.</h1><p>Draw shapes or turn a word into a route, and explore your city in a new way.</p></div><form id="plan-form"><div class="field"><label for="start">Starting point</label><div class="inputwrap">${icon("map-pin")}<input id="start" name="start" value="${escapeHtml(request.start)}" required autocomplete="off" placeholder="Latitude, longitude"></div><button type="button" class="textbutton" data-action="location">${icon("navigation-arrow")} Use my location</button><p id="location-status" class="helper">Location is requested only when you choose. You can always enter coordinates or click the map.</p></div><div class="activities" role="group" aria-label="Activity">${(["Walking", "Running", "Cycling"] as const).map((activity) => `<button type="button" class="activity ${request.activity === activity ? "selected" : ""}" data-activity="${activity}" aria-pressed="${request.activity === activity}">${icon(activity === "Walking" ? "person-simple-walk" : activity === "Running" ? "person-simple-run" : "bicycle")}${activity}</button>`).join("")}</div><div id="design-picker">${designPicker(state)}</div><div class="between" style="margin-top:-20px;margin-bottom:20px"><span class="small muted">${escapeHtml(request.design)} selected</span>${link("catalogue", `All designs ${icon("arrow-right")}`)}</div><div class="field"><label for="distance">Target distance</label><div class="inputwrap"><input id="distance" name="distance" type="number" value="${request.distanceKm}" min="1" max="100" step="0.1" required><span>km</span></div><p id="distance-help" class="helper">±${request.tolerancePercent}% · ${bounds.minimumKm.toFixed(1)}–${bounds.maximumKm.toFixed(1)} km · 1–100 km allowed</p></div><div class="settings"><label class="between small"><strong>Return to start</strong><span class="toggle"><input type="checkbox" id="loop" ${request.returnsToStart ? "checked" : ""} aria-label="Return to start"></span></label></div><details><summary class="small muted" style="cursor:pointer;margin-bottom:16px">Distance flexibility</summary><div class="row"><label for="tolerance">Allowed difference</label><select id="tolerance">${[5, 10, 20].map((value) => `<option value="${value}" ${request.tolerancePercent === value ? "selected" : ""}>±${value}%</option>`).join("")}</select></div></details>${workflowIssue(workflow?.issue ?? null)}<button class="primary full" type="submit" ${busy ? "disabled" : ""}>${icon(busy ? "circle-notch" : "sparkle")} ${busy ? "Checking start…" : "Generate routes"}</button><p class="helper">The server validates coverage and road access before you can confirm and start generation.</p></form>${plannerRuntime(workflow, request.revision)}`;
}

function catalogue(state: AppState): string {
  return `<div class="catalogue">${top("What will you draw?", "Choose a shape or a ready-made word. Local fit is checked only by the routing service.")}${designPicker(state, true)}<div class="notice">Some designs need more space or repeated sections. Every real connecting section will count toward distance.</div><a class="primary full" href="#/planner">Use ${escapeHtml(state.request.design)} ${icon("arrow-right")}</a><p class="helper">Custom words are planned for a later release.</p></div>`;
}

function generating(state: AppState, workflow: PlannerWorkflowSnapshot | null): string {
  const activeState = workflow?.request ? { ...state, request: workflow.request } : state;
  const stages = ["checking_roads", "fitting_design", "comparing_routes"] as const;
  const current = workflow?.status?.stage ?? "queued";
  const currentIndex = stages.indexOf(current as (typeof stages)[number]);
  const labels = ["Checking roads", "Fitting your design", "Comparing routes"];
  const items = stages.map((_stage, index) => {
    const className = index < currentIndex ? "done" : index === currentIndex ? "done pulse" : "pending";
    const marker = index <= currentIndex ? "check-circle" : "circle";
    return `<li class="${className}">${icon(marker)} ${labels[index]}</li>`;
  }).join("");
  return `${top("Finding your next adventure.", "Only observed service stages are shown; no percentage is estimated.")}${summary(activeState)}<ol class="stage-list" aria-live="polite">${items}</ol><p class="small muted">Your active request is private to this guest session and can resume after a refresh.</p>${workflowIssue(workflow?.issue ?? null)}<button type="button" class="secondary full" style="margin-top:24px" data-action="cancel-job">Cancel search</button>`;
}

function unavailableResult(state: AppState, details: boolean): string {
  return `${top(details ? "Route details" : "A few ways to make it yours.", details ? "A validated candidate is required before details can be shown." : "Validated route candidates will appear here.", details ? "results" : "planner")}${summary(state)}<span class="hero-icon">${icon("map-trifold")}</span><div class="notice">No route candidate is displayed. GPSART never creates geometry, connectivity, distance, access, instructions, or success metrics in the browser.</div>${link("planner", "Refine your plan")}`;
}

function saved(loaded: LoadedSnapshots): string {
  let content: string;
  if (loaded.kind === "unsupported_snapshot") content = `<div class="notice">A saved route uses an older, malformed, or unsupported format. It has not been loaded or changed.</div>`;
  else if (loaded.kind === "unavailable") content = `<div class="notice">Browser storage is unavailable. No route data has been changed.</div>`;
  else if (!loaded.snapshots.length) content = `<div class="empty-space">${icon("bookmark-simple")}<h2 style="margin:18px 0 12px">A little inspiration for later.</h2><p class="muted small">Save a validated route and it will be waiting here for your next outing.</p><a class="primary full" style="margin-top:28px" href="#/planner">Plan your first route ${icon("arrow-right")}</a></div>`;
  else content = loaded.snapshots.map((snapshot) => `<article class="saved-row"><span class="eyebrow">Validated route snapshot v${snapshot.version}</span><h3 style="margin-top:10px">${escapeHtml(snapshot.id)}</h3><p class="small muted">${(snapshot.totalMetres / 1000).toFixed(1)} km · ${snapshot.trace.length} recorded points</p><p class="helper">Full route details become available with the route-detail service.</p></article>`).join("");
  return `${top("Your next adventures.", "Saved on this device. No account needed.")}<div class="notice">These routes stay in this browser. Clearing site data removes them.</div>${content}`;
}

function failure(screen: "no-match" | "coverage" | "error", state: AppState): string {
  const copy = screen === "coverage" ? ["Outside our pilot area", "Coverage must be confirmed by the service.", "map-trifold"] : screen === "error" ? ["The route service is unavailable.", "Your inputs are safe and unchanged.", "cloud-slash"] : ["We couldn’t find a fit this time.", "No qualifying route was returned within the search budget.", "path"];
  return `${top(copy[0], copy[1])}<span class="hero-icon">${icon(copy[2])}</span>${summary(state)}${screen === "no-match" ? `<h3>Try a different approach</h3><p class="notice">Change the distance or design. Suggestions are not treated as routing facts until checked by the service.</p>` : `<div class="notice">${screen === "coverage" ? "No coverage is assumed in the browser." : "Try again when the service is available."}</div>`}${link("planner", "Edit your original request")}`;
}

function auth(screen: Screen, state: AppState): string {
  let content: string;
  if (screen === "login") content = `<span class="eyebrow">A little more connected</span><h1>Your next adventure starts here.</h1><p>Sign in to make GPSART yours. You can always plan and save routes on this device as a guest.</p><button class="google" type="button" data-action="google"><img src="/assets/google-logo.png" alt="">Continue with Google</button><div class="or">or</div><a class="secondary full" href="#/planner">Continue as guest ${icon("arrow-right")}</a><ul class="auth-checks"><li>${icon("check")} No password to remember</li><li>${icon("lock-simple")} Your routes are private by default</li><li>${icon("map-pin")} Location is requested only when you choose</li></ul><div class="demo-strip">Design preview: authentication is not connected. No Google account is accessed.</div>`;
  else if (screen === "auth-pending") content = `<span class="hero-icon">${icon("circle-notch")}</span><h1>Connecting with Google.</h1><p>No authentication request is sent in this foundation build.</p><div role="status" class="notice">Authentication unavailable.</div>${link("login", "Cancel sign-in")}`;
  else if (screen === "auth-error") content = `<span class="hero-icon">${icon("warning-circle")}</span><h1>Let’s try that again.</h1><p>Sign-in could not be completed. Your plan and saved routes are still here.</p>${link("login", "Return to sign in")}`;
  else if (screen === "welcome") content = `<span class="hero-icon">${icon("check-circle")}</span><span class="eyebrow">You’re ready to explore</span><h1>Welcome to GPSART.</h1><p>Your next route is only a few choices away.</p>${link("planner", "Start planning")}`;
  else content = `<div class="avatar">${icon("user")}</div><span class="eyebrow">${state.signedIn ? "Account" : "Guest"}</span><h1>Your corner of GPSART.</h1><p>Account services are not connected in this build.</p><div class="rows"><div class="row"><span>Route storage</span><strong>On this device</strong></div><div class="row"><span>Cloud sync</span><span>Not connected</span></div></div><a class="primary full" href="#/saved">View saved routes</a>`;
  return `<main id="content" class="auth-layout"><section class="auth-form"><div class="auth-inner">${content}</div></section><aside class="auth-art" aria-label="Illustrative Bengaluru map"><img src="/assets/map.jpg" alt="Pale illustrative street map of Bengaluru"><div class="auth-quote"><span class="eyebrow">Take the scenic shape</span><h2 style="margin-top:12px">Make your miles<br>a little more creative.</h2><p class="small muted">Illustrative artwork · not a navigable route.</p></div></aside></main>`;
}

function screenIndex(): string {
  return `<main id="content" class="screen-index"><span class="eyebrow">GPSART · Design review</span><h1>Every step of the adventure.</h1><p class="muted">Review each state without simulating backend success.</p><div class="screen-list">${screens.filter((screen) => screen !== "screens").map((screen) => `<a class="screen-link" href="#/${screen}"><h3>${screen}</h3><p>Open the ${screen} state.</p></a>`).join("")}</div></main>`;
}

export function isStandaloneScreen(screen: Screen): boolean {
  return ["login", "auth-pending", "auth-error", "welcome", "account", "screens"].includes(screen);
}

export function renderScreen(screen: Screen, state: AppState, loaded: LoadedSnapshots, workflow: PlannerWorkflowSnapshot | null = null): string {
  if (screen === "planner") return planner(state, workflow);
  if (screen === "catalogue") return catalogue(state);
  if (screen === "generating") return generating(state, workflow);
  if (screen === "results") return unavailableResult(state, false);
  if (screen === "details") return unavailableResult(state, true);
  if (screen === "saved") return saved(loaded);
  if (screen === "no-match" || screen === "coverage" || screen === "error") return failure(screen, state);
  if (screen === "screens") return screenIndex();
  return auth(screen, state);
}
