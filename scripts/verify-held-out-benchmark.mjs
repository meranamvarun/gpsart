import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const SHA256 = /^[a-f0-9]{64}$/;
const VALIDITY_CHECKS = [
  "continuity",
  "exact_confirmed_start",
  "access_direction_turn_policy",
  "distance_tolerance",
  "connector_retrace_disclosure",
  "immutable_versions",
  "authorization",
  "gpx_round_trip",
];
const FORBIDDEN_KEYS = new Set([
  "latitude",
  "longitude",
  "coordinates",
  "requested_point",
  "confirmed_start",
  "geometry",
  "trace",
  "cookie",
  "cookies",
  "token",
  "access_token",
  "refresh_token",
  "credential",
  "credentials",
  "password",
]);

const record = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const nonEmpty = (value) => typeof value === "string" && value.trim().length > 0;

function findForbidden(value, path = "package", findings = []) {
  if (Array.isArray(value)) {
    value.forEach((entry, index) => findForbidden(entry, `${path}[${index}]`, findings));
    return findings;
  }
  if (!record(value)) return findings;
  for (const [key, entry] of Object.entries(value)) {
    if (FORBIDDEN_KEYS.has(key.toLowerCase())) findings.push(`${path}.${key}`);
    findForbidden(entry, `${path}.${key}`, findings);
  }
  return findings;
}

function exactKeys(value, keys) {
  return record(value)
    && Object.keys(value).length === keys.length
    && keys.every((key) => Object.hasOwn(value, key));
}

function rejectUnexpectedKeys(value, keys, path, errors) {
  if (!record(value)) return;
  for (const key of Object.keys(value)) {
    if (!keys.includes(key)) errors.push(`${path} has unexpected field: ${key}`);
  }
}

function routeIsValid(output) {
  return record(output.validity)
    && exactKeys(output.validity, VALIDITY_CHECKS)
    && VALIDITY_CHECKS.every((key) => output.validity[key] === true);
}

function close(left, right) {
  return typeof left === "number" && Number.isFinite(left) && Math.abs(left - right) < 1e-12;
}

export function validateBenchmarkPackage(packageValue) {
  const errors = [];
  if (!record(packageValue)) return ["benchmark package must be a JSON object"];
  rejectUnexpectedKeys(packageValue, [
    "schema_version", "frozen_at", "executed_at", "selection_plan_sha256", "artifacts",
    "cases", "outputs", "reviews", "exclusions", "aggregate",
  ], "package", errors);
  if (packageValue.schema_version !== "gpsart-held-out-benchmark-v2") errors.push("schema_version must be gpsart-held-out-benchmark-v2");

  const frozen = Date.parse(packageValue.frozen_at);
  const executed = Date.parse(packageValue.executed_at);
  if (!Number.isFinite(frozen) || !Number.isFinite(executed) || frozen >= executed) errors.push("the case set must be frozen before execution");
  if (!SHA256.test(packageValue.selection_plan_sha256 ?? "")) errors.push("selection_plan_sha256 must pin the preregistered selection plan");

  const artifactKeys = ["source_revision", "client_sha256", "api_sha256", "worker_sha256", "routing_manifest_sha256"];
  const versionKeys = ["map_data_version", "configuration_version", "algorithm_version"];
  if (!record(packageValue.artifacts)
    || artifactKeys.some((key) => !SHA256.test(packageValue.artifacts[key] ?? ""))
    || versionKeys.some((key) => !nonEmpty(packageValue.artifacts[key]))) {
    errors.push("artifacts must contain immutable source, component, routing, map, configuration, and algorithm pins");
  }

  for (const path of findForbidden(packageValue)) errors.push(`privacy-forbidden field: ${path}`);

  const cases = Array.isArray(packageValue.cases) ? packageValue.cases : [];
  const outputs = Array.isArray(packageValue.outputs) ? packageValue.outputs : [];
  const reviews = Array.isArray(packageValue.reviews) ? packageValue.reviews : [];
  const exclusions = Array.isArray(packageValue.exclusions) ? packageValue.exclusions : null;
  if (!Array.isArray(packageValue.cases)) errors.push("cases must be an array");
  if (!Array.isArray(packageValue.outputs)) errors.push("outputs must be an array");
  if (!Array.isArray(packageValue.reviews)) errors.push("reviews must be an array");
  if (exclusions === null || exclusions.length !== 0) errors.push("post-execution exclusions are forbidden; every frozen case must remain in the aggregate");

  const ids = new Set();
  for (const entry of cases) {
    if (!record(entry)
      || !nonEmpty(entry.id)
      || !["feasible", "difficult_or_no_match"].includes(entry.cohort)
      || !["shape", "text"].includes(entry.category)
      || !nonEmpty(entry.intended_design)
      || !nonEmpty(entry.start_ref)
      || !nonEmpty(entry.provenance)
      || !nonEmpty(entry.selection_rule)
      || !SHA256.test(entry.reference_traversal_sha256 ?? "")) {
      errors.push("every case must have a valid identity, cohort, category, provenance, selection rule, start reference, design, and reference traversal pin");
      continue;
    }
    if (ids.has(entry.id)) errors.push(`case id must be unique: ${entry.id}`);
    ids.add(entry.id);
  }

  const feasible = cases.filter(({ cohort }) => cohort === "feasible");
  const shapes = feasible.filter(({ category }) => category === "shape");
  const texts = feasible.filter(({ category }) => category === "text");
  const difficult = cases.filter(({ cohort }) => cohort === "difficult_or_no_match");
  if (feasible.length < 50) errors.push("at least 50 feasible cases are required");
  if (shapes.length < 20) errors.push("at least 20 feasible shape cases are required");
  if (texts.length < 20) errors.push("at least 20 feasible text cases are required");
  if (difficult.length < 20) errors.push("at least 20 difficult or no-match cases are required");

  const outputsByCase = new Map();
  for (const output of outputs) {
    if (!record(output) || !nonEmpty(output.case_id) || !["match", "no_match", "error"].includes(output.outcome) || !SHA256.test(output.output_sha256 ?? "")) {
      errors.push("every output must have a case id, classified outcome, and digest");
      continue;
    }
    const entries = outputsByCase.get(output.case_id) ?? [];
    entries.push(output);
    outputsByCase.set(output.case_id, entries);
    if (!ids.has(output.case_id)) errors.push(`output references an unknown case: ${output.case_id}`);
    if (output.outcome === "match") {
      if (!nonEmpty(output.normalized_artifact)) errors.push(`matched output ${output.case_id} must retain a normalized review artifact`);
      if (!routeIsValid(output)) errors.push(`returned route ${output.case_id} must pass all validity checks`);
    } else if (output.validity !== undefined || output.normalized_artifact !== undefined) {
      errors.push(`non-match output ${output.case_id} cannot claim route validity or a route artifact`);
    }
  }
  for (const entry of cases) {
    if ((outputsByCase.get(entry.id) ?? []).length !== 1) errors.push(`every case must have exactly one output: ${entry.id}`);
  }

  const reviewsByCase = new Map();
  const reviewKeys = new Set();
  for (const review of reviews) {
    if (!record(review) || !nonEmpty(review.case_id) || !nonEmpty(review.reviewer) || !nonEmpty(review.selected_design) || !SHA256.test(review.options_sha256 ?? "")) {
      errors.push("every review must identify its case, reviewer, fixed options, and selected design");
      continue;
    }
    const caseValue = cases.find(({ id }) => id === review.case_id);
    if (!caseValue || caseValue.cohort !== "feasible") errors.push(`reviews may reference feasible cases only: ${review.case_id}`);
    const key = `${review.case_id}\0${review.reviewer}`;
    if (reviewKeys.has(key)) errors.push(`reviewer decisions must be unique per case: ${review.case_id}`);
    reviewKeys.add(key);
    const entries = reviewsByCase.get(review.case_id) ?? [];
    entries.push(review);
    reviewsByCase.set(review.case_id, entries);
  }

  const qualified = new Set();
  for (const entry of feasible) {
    const caseReviews = reviewsByCase.get(entry.id) ?? [];
    const reviewers = new Set(caseReviews.map(({ reviewer }) => reviewer));
    if (caseReviews.length !== 3 || reviewers.size !== 3) errors.push(`feasible case ${entry.id} requires three distinct reviewers`);
    if (caseReviews.some(({ blind, independent }) => blind !== true || independent !== true)) errors.push(`all reviews for ${entry.id} must be blind and independent`);
    const options = new Set(caseReviews.map(({ options_sha256 }) => options_sha256));
    if (options.size > 1) errors.push(`all reviewers for ${entry.id} must receive the same fixed alternatives`);
    const recognitions = caseReviews.filter((review) => review.blind === true && review.independent === true && review.selected_design === entry.intended_design).length;
    const output = (outputsByCase.get(entry.id) ?? [])[0];
    if (recognitions >= 2 && output?.outcome === "match" && routeIsValid(output)) qualified.add(entry.id);
  }

  const shapeQualified = shapes.filter(({ id }) => qualified.has(id)).length;
  const textQualified = texts.filter(({ id }) => qualified.has(id)).length;
  const shapeRate = shapes.length ? shapeQualified / shapes.length : 0;
  const textRate = texts.length ? textQualified / texts.length : 0;
  const invalidReturnedRoutes = outputs.filter((output) => output?.outcome === "match" && !routeIsValid(output)).length;
  if (shapeRate < 0.8) errors.push(`shape qualifying rate ${shapeRate} is below 0.8`);
  if (textRate < 0.8) errors.push(`text qualifying rate ${textRate} is below 0.8`);
  if (invalidReturnedRoutes > 0) errors.push("one or more returned routes is invalid");

  const aggregate = packageValue.aggregate;
  rejectUnexpectedKeys(aggregate, [
    "feasible_total", "shape_total", "text_total", "difficult_total", "shape_qualified",
    "text_qualified", "shape_rate", "text_rate", "invalid_returned_routes", "thresholds_met",
  ], "aggregate", errors);
  const expectedAggregate = {
    feasible_total: feasible.length,
    shape_total: shapes.length,
    text_total: texts.length,
    difficult_total: difficult.length,
    shape_qualified: shapeQualified,
    text_qualified: textQualified,
    shape_rate: shapeRate,
    text_rate: textRate,
    invalid_returned_routes: invalidReturnedRoutes,
    thresholds_met: feasible.length >= 50 && shapes.length >= 20 && texts.length >= 20 && difficult.length >= 20
      && shapeRate >= 0.8 && textRate >= 0.8 && invalidReturnedRoutes === 0 && errors.length === 0,
  };
  if (!record(aggregate)
    || Object.entries(expectedAggregate).some(([key, value]) => typeof value === "number" ? !close(aggregate[key], value) : aggregate[key] !== value)) {
    errors.push("aggregate must exactly match the fail-closed calculation");
  }
  return errors;
}

const EXTERNAL_AUTHENTICATION = Object.freeze([
  "case provenance and preregistration time",
  "reference-traversal feasibility and artifact digests",
  "raw output and normalized-artifact equivalence",
  "reviewer identity, blindness, independence, and decisions",
  "validity evidence and production artifact identity",
  "independent QA signature",
]);

export function assessBenchmarkPackage(packageValue) {
  const errors = validateBenchmarkPackage(packageValue);
  return {
    structural_preflight_passed: errors.length === 0,
    release_gate_passed: false,
    requires_external_authentication: [...EXTERNAL_AUTHENTICATION],
    errors,
  };
}

async function main() {
  const [mode, path] = process.argv.slice(2);
  if (mode !== "--preflight" || !path || process.argv.length !== 4) throw new Error("usage: node scripts/verify-held-out-benchmark.mjs --preflight <evidence-package.json>");
  const packageValue = JSON.parse(await readFile(resolve(path), "utf8"));
  const assessment = assessBenchmarkPackage(packageValue);
  if (assessment.errors.length) throw new Error(`Held-out benchmark structural preflight failed:\n${assessment.errors.map((error) => `- ${error}`).join("\n")}`);
  console.log(`Structural preflight passed; release gate NOT PASSED. External authentication remains required for ${assessment.requires_external_authentication.join(", ")}.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
