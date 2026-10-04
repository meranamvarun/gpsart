import type { DirectionMarker, Position, RouteInstruction, RouteSnapshot } from "../domain/route-snapshot.ts";

export const ROUTE_USE_ADVISORY = "Routing and map data can be incomplete or change. Before and during the activity, verify that every segment is currently open and lawful for your activity; obey posted signs, closures, property restrictions, and applicable laws; and choose a safe alternative or stop when conditions differ.";

export interface PresentedCandidate {
  readonly snapshot: RouteSnapshot;
  readonly distanceDifferenceMetres: number;
  readonly artworkPercent: number;
  readonly connectorPercent: number;
  readonly repeatedPercent: number;
}

export interface CandidateComparison {
  readonly candidates: readonly PresentedCandidate[];
  readonly targetMetres: number;
  readonly bestShapeId: string | null;
  readonly closestDistanceId: string | null;
}

export interface RouteMapModel {
  readonly northUp: true;
  readonly highlighted: {
    readonly id: string;
    readonly trace: readonly Position[];
    readonly directionMarkers: readonly DirectionMarker[];
  };
  readonly alternatives: readonly { readonly id: string; readonly trace: readonly Position[] }[];
  readonly idealDesignOverlay: null;
}

const escapeHtml = (value: string): string => value.replace(/[&<>'"]/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" })[character] ?? character);
const percent = (part: number, total: number): number => Math.round(part * 100 / total);
const kilometres = (metres: number): string => `${(metres / 1000).toFixed(1)} km`;
const minutes = (seconds: number): string => `${Math.ceil(seconds / 60)} min`;

function geometryIdentity(snapshot: RouteSnapshot): string {
  return JSON.stringify(snapshot.trace.map(({ longitude, latitude }) => [longitude, latitude]));
}

export function buildCandidateComparison(snapshots: readonly RouteSnapshot[], targetMetres: number): CandidateComparison {
  const seenIds = new Set<string>();
  const seenGeometry = new Set<string>();
  const distinct = snapshots.filter((snapshot) => {
    const identity = geometryIdentity(snapshot);
    if (seenIds.has(snapshot.id) || seenGeometry.has(identity)) return false;
    seenIds.add(snapshot.id);
    seenGeometry.add(identity);
    return true;
  }).slice(0, 3);
  const candidates = distinct.map((snapshot): PresentedCandidate => Object.freeze({
    snapshot,
    distanceDifferenceMetres: Math.abs(snapshot.totalMetres - targetMetres),
    artworkPercent: percent(snapshot.artworkMetres, snapshot.totalMetres),
    connectorPercent: percent(snapshot.connectorMetres, snapshot.totalMetres),
    repeatedPercent: percent(snapshot.repeatedMetres, snapshot.totalMetres),
  }));
  const closest = [...candidates].sort((left, right) => left.distanceDifferenceMetres - right.distanceDifferenceMetres || left.snapshot.id.localeCompare(right.snapshot.id))[0];
  const bestShape = [...candidates].sort((left, right) => {
    const qualityDifference = Number(right.snapshot.qualityBand === "strong_match") - Number(left.snapshot.qualityBand === "strong_match");
    return qualityDifference || right.artworkPercent - left.artworkPercent || left.repeatedPercent - right.repeatedPercent || left.snapshot.id.localeCompare(right.snapshot.id);
  })[0];
  return Object.freeze({
    candidates: Object.freeze(candidates),
    targetMetres,
    bestShapeId: bestShape?.snapshot.id ?? null,
    closestDistanceId: closest?.snapshot.id ?? null,
  });
}

function selectedCandidate(comparison: CandidateComparison, selectedId: string): PresentedCandidate | null {
  return comparison.candidates.find(({ snapshot }) => snapshot.id === selectedId) ?? comparison.candidates[0] ?? null;
}

function candidateLabels(comparison: CandidateComparison, id: string): string {
  const labels = [
    comparison.bestShapeId === id ? "Best shape match" : null,
    comparison.closestDistanceId === id ? "Closest distance" : null,
  ].filter((label): label is string => label !== null);
  return labels.map((label) => `<span class="tag">${label}</span>`).join(" · ");
}

export function renderCandidateResults(comparison: CandidateComparison, selectedId: string): string {
  if (!comparison.candidates.length) return `<div class="notice" role="status">No complete route snapshot is available. No geometry or metrics are shown.</div>`;
  return `<div class="candidate-list" aria-label="Route candidates">${comparison.candidates.map((candidate, index) => {
    const snapshot = candidate.snapshot;
    const selected = snapshot.id === selectedId || (!comparison.candidates.some(({ snapshot: item }) => item.id === selectedId) && index === 0);
    return `<button type="button" class="route-card${selected ? " selected" : ""}" data-candidate-id="${escapeHtml(snapshot.id)}" aria-pressed="${selected}"><span class="eyebrow">Route ${index + 1}</span><span class="distance">${kilometres(snapshot.totalMetres)}</span><span class="route-tags">${candidateLabels(comparison, snapshot.id)}</span><span class="route-meta"><span>${snapshot.qualityBand === "strong_match" ? "Strong" : "Approximate"} shape match</span><span>${candidate.artworkPercent}% artwork</span><span>${candidate.connectorPercent}% connectors</span><span>${candidate.repeatedPercent}% repeated</span><span>${minutes(snapshot.durationSeconds)}</span></span></button>`;
  }).join("")}</div>`;
}

function endpointPolicy(snapshot: RouteSnapshot): string {
  return snapshot.start.latitude === snapshot.finish.latitude && snapshot.start.longitude === snapshot.finish.longitude
    ? "Return to start"
    : "Separate finish";
}

function instructionText(instruction: RouteInstruction, index: number): string {
  if (instruction.kind === "depart") return `Depart from route point ${instruction.geometryIndex + 1}`;
  if (instruction.kind === "arrive") return `Arrive at route point ${instruction.geometryIndex + 1}`;
  if (instruction.kind === "via") return `Continue via route point ${instruction.geometryIndex + 1}`;
  return `Provider manoeuvre code ${instruction.providerCode} at route point ${instruction.geometryIndex + 1}`;
}

export function renderCandidateDetails(comparison: CandidateComparison, selectedId: string, baseMapFailed = false): string {
  const candidate = selectedCandidate(comparison, selectedId);
  if (!candidate) return `<div class="notice" role="status">Route details are unavailable because no complete snapshot was loaded.</div>`;
  const snapshot = candidate.snapshot;
  const traversalDirections = snapshot.traversals.map((traversal, index) => `<li><strong>${escapeHtml(traversal.class)}</strong>: ${escapeHtml(traversal.direction)} · ${traversal.distanceMetres} m${traversal.retraced ? " · retraced" : ""} · points ${traversal.beginGeometryIndex + 1}–${traversal.endGeometryIndex + 1}</li>`).join("");
  const instructions = snapshot.instructions.map((instruction, index) => `<li>${escapeHtml(instructionText(instruction, index))}</li>`).join("");
  const attribution = snapshot.provenance.attribution ? ` · ${escapeHtml(snapshot.provenance.attribution)}` : "";
  return `${baseMapFailed ? `<div class="notice" role="status"><strong>Base map unavailable.</strong> The verified route geometry and all route facts remain available below.</div>` : ""}<section aria-labelledby="route-facts-title"><h2 id="route-facts-title">Route facts</h2><div class="metrics"><div class="metric"><strong>${kilometres(snapshot.totalMetres)}</strong><span>Total distance</span></div><div class="metric"><strong>${minutes(snapshot.durationSeconds)}</strong><span>Estimated duration from ${escapeHtml(snapshot.versions.profile)}</span></div></div><div class="rows"><div class="row"><span>Artwork distance</span><strong>${snapshot.artworkMetres} m (${candidate.artworkPercent}%)</strong></div><div class="row"><span>Connectors</span><strong>${snapshot.connectorMetres} m (${candidate.connectorPercent}%)</strong></div><div class="row"><span>Repeated distance</span><strong>${snapshot.repeatedMetres} m (${candidate.repeatedPercent}%) · included in total</strong></div><div class="row"><span>Endpoint policy</span><strong>${endpointPolicy(snapshot)}</strong></div><div class="row"><span>Shape quality</span><strong>${snapshot.qualityBand === "strong_match" ? "Strong match" : "Approximate match"}</strong></div><div class="row"><span>Provider</span><strong>${escapeHtml(snapshot.provenance.routingProvider)}${attribution}</strong></div></div><h3>Complete traversal and direction</h3><ol class="instructions route-traversals">${traversalDirections}</ol><h3>Instructions</h3><ol class="instructions">${instructions}</ol><div class="notice"><strong>Routed geometry</strong> is the complete ordered path shown on the map. Ideal design overlay is not shown. If an optional design overlay is added later, it must be labelled and cannot replace this route.</div><div class="notice">${escapeHtml(ROUTE_USE_ADVISORY)}</div><details><summary>Snapshot provenance</summary><div class="rows"><div class="row"><span>Generated</span><strong>${new Date(snapshot.generatedAtEpochSeconds * 1000).toISOString()}</strong></div><div class="row"><span>Routing data</span><strong>${escapeHtml(snapshot.versions.routingData)}</strong></div><div class="row"><span>Routing policy</span><strong>${escapeHtml(snapshot.versions.routingPolicy)}</strong></div><div class="row"><span>Algorithm</span><strong>${escapeHtml(snapshot.versions.algorithm)}</strong></div><div class="row"><span>Snapshot</span><strong>${escapeHtml(snapshot.versions.snapshot)}</strong></div></div></details></section>`;
}

export function routeMapModel(comparison: CandidateComparison, selectedId: string): RouteMapModel {
  const candidate = selectedCandidate(comparison, selectedId);
  if (!candidate) throw new Error("a complete candidate is required for a route map");
  return Object.freeze({
    northUp: true,
    highlighted: Object.freeze({
      id: candidate.snapshot.id,
      trace: candidate.snapshot.trace,
      directionMarkers: candidate.snapshot.directionMarkers,
    }),
    alternatives: Object.freeze(comparison.candidates.filter(({ snapshot }) => snapshot.id !== candidate.snapshot.id).map(({ snapshot }) => Object.freeze({ id: snapshot.id, trace: snapshot.trace }))),
    idealDesignOverlay: null,
  });
}
