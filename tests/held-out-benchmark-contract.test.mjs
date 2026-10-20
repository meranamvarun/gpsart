import assert from "node:assert/strict";
import test from "node:test";

import { assessBenchmarkPackage, validateBenchmarkPackage } from "../scripts/verify-held-out-benchmark.mjs";

const digest = (character) => character.repeat(64);

function validPackage() {
  const cases = [];
  for (let index = 0; index < 25; index += 1) {
    cases.push({
      id: `synthetic-shape-${index}`,
      cohort: "feasible",
      category: "shape",
      intended_design: "shape-a",
      start_ref: `reviewed-start-${index}`,
      provenance: "synthetic validator test only",
      selection_rule: "fixed before execution",
      reference_traversal_sha256: digest("a"),
    });
    cases.push({
      id: `synthetic-text-${index}`,
      cohort: "feasible",
      category: "text",
      intended_design: "text-a",
      start_ref: `reviewed-start-${index}`,
      provenance: "synthetic validator test only",
      selection_rule: "fixed before execution",
      reference_traversal_sha256: digest("b"),
    });
  }
  for (let index = 0; index < 20; index += 1) {
    cases.push({
      id: `synthetic-difficult-${index}`,
      cohort: "difficult_or_no_match",
      category: index % 2 ? "shape" : "text",
      intended_design: index % 2 ? "shape-a" : "text-a",
      start_ref: `restricted-start-${index}`,
      provenance: "synthetic validator test only",
      selection_rule: "fixed before execution",
      reference_traversal_sha256: digest("c"),
    });
  }
  const outputs = cases.map((entry) => entry.cohort === "feasible" ? {
    case_id: entry.id,
    outcome: "match",
    normalized_artifact: `review/${entry.id}.svg`,
    output_sha256: digest("d"),
    validity: {
      continuity: true,
      exact_confirmed_start: true,
      access_direction_turn_policy: true,
      distance_tolerance: true,
      connector_retrace_disclosure: true,
      immutable_versions: true,
      authorization: true,
      gpx_round_trip: true,
    },
  } : { case_id: entry.id, outcome: "no_match", output_sha256: digest("e") });
  const reviews = cases.filter(({ cohort }) => cohort === "feasible").flatMap((entry) =>
    ["reviewer-a", "reviewer-b", "reviewer-c"].map((reviewer) => ({
      case_id: entry.id,
      reviewer,
      independent: true,
      blind: true,
      options_sha256: digest("f"),
      selected_design: entry.intended_design,
    })),
  );
  return {
    schema_version: "gpsart-held-out-benchmark-v2",
    frozen_at: "2026-10-20T00:00:00Z",
    executed_at: "2026-10-20T01:00:00Z",
    selection_plan_sha256: digest("1"),
    artifacts: {
      source_revision: digest("2"),
      client_sha256: digest("3"),
      api_sha256: digest("4"),
      worker_sha256: digest("5"),
      routing_manifest_sha256: digest("6"),
      map_data_version: "synthetic-map-v1",
      configuration_version: "synthetic-config-v1",
      algorithm_version: "synthetic-algorithm-v1",
    },
    cases,
    outputs,
    reviews,
    exclusions: [],
    aggregate: {
      feasible_total: 50,
      shape_total: 25,
      text_total: 25,
      difficult_total: 20,
      shape_qualified: 25,
      text_qualified: 25,
      shape_rate: 1,
      text_rate: 1,
      invalid_returned_routes: 0,
      thresholds_met: true,
    },
  };
}

test("accepts a structurally complete package for external authentication", () => {
  assert.deepEqual(validateBenchmarkPackage(validPackage()), []);
  assert.deepEqual(assessBenchmarkPackage(validPackage()), {
    structural_preflight_passed: true,
    release_gate_passed: false,
    requires_external_authentication: [
      "case provenance and preregistration time",
      "reference-traversal feasibility and artifact digests",
      "raw output and normalized-artifact equivalence",
      "reviewer identity, blindness, independence, and decisions",
      "validity evidence and production artifact identity",
      "independent QA signature",
    ],
    errors: [],
  });
});

test("fabricated assertions can never be represented as a release-gate pass", () => {
  const packageValue = validPackage();
  packageValue.selection_plan_sha256 = digest("0");
  packageValue.artifacts = Object.fromEntries(Object.entries(packageValue.artifacts).map(([key, value]) => [key, key.endsWith("version") ? `synthetic-${key}` : digest("0")]));
  for (const entry of packageValue.cases) entry.reference_traversal_sha256 = digest("0");
  for (const output of packageValue.outputs) output.output_sha256 = digest("0");
  for (const review of packageValue.reviews) review.options_sha256 = digest("0");
  const assessment = assessBenchmarkPackage(packageValue);
  assert.equal(assessment.release_gate_passed, false);
  assert.ok(assessment.requires_external_authentication.length > 0);

  packageValue.release_gate_passed = true;
  packageValue.passed = true;
  packageValue.aggregate.release_gate_passed = true;
  const embeddedClaims = assessBenchmarkPackage(packageValue);
  assert.equal(embeddedClaims.structural_preflight_passed, false);
  assert.match(embeddedClaims.errors.join("\n"), /unexpected field/);
});

test("fails closed on missing cases, outputs, reviews, or post-execution exclusions", () => {
  const tooSmall = validPackage();
  tooSmall.cases = tooSmall.cases.slice(1);
  assert.match(validateBenchmarkPackage(tooSmall).join("\n"), /every case|50 feasible/);

  const missingOutput = validPackage();
  missingOutput.outputs.pop();
  assert.match(validateBenchmarkPackage(missingOutput).join("\n"), /exactly one output/);

  const missingReview = validPackage();
  missingReview.reviews.pop();
  assert.match(validateBenchmarkPackage(missingReview).join("\n"), /three distinct reviewers/);

  const excluded = validPackage();
  excluded.exclusions.push({ case_id: excluded.cases[0].id, reason: "result was inconvenient" });
  assert.match(validateBenchmarkPackage(excluded).join("\n"), /exclusions/);
});

test("rejects non-blind or non-independent reviews and preserves disagreement", () => {
  const packageValue = validPackage();
  packageValue.reviews[0].blind = false;
  packageValue.reviews[1].independent = false;
  packageValue.reviews[2].selected_design = "shape-b";
  const errors = validateBenchmarkPackage(packageValue).join("\n");
  assert.match(errors, /blind and independent/);
  assert.match(errors, /aggregate/);
});

test("one invalid returned route fails the gate", () => {
  const packageValue = validPackage();
  packageValue.outputs[0].validity.authorization = false;
  const errors = validateBenchmarkPackage(packageValue).join("\n");
  assert.match(errors, /all validity checks/);
  assert.match(errors, /aggregate/);
});

test("rejects exact coordinates, credentials, cookies, tokens, and trace arrays anywhere", () => {
  for (const [key, value] of [
    ["latitude", 12.34],
    ["longitude", 56.78],
    ["cookie", "secret"],
    ["token", "secret"],
    ["trace", [[1, 2]]],
  ]) {
    const packageValue = validPackage();
    packageValue.outputs[0][key] = value;
    assert.match(validateBenchmarkPackage(packageValue).join("\n"), /privacy-forbidden field/);
  }
});

test("recomputes category rates and requires the 80 percent threshold", () => {
  const packageValue = validPackage();
  for (let index = 0; index < 6; index += 1) {
    const id = `synthetic-text-${index}`;
    for (const review of packageValue.reviews.filter((entry) => entry.case_id === id)) {
      review.selected_design = "shape-a";
    }
  }
  const errors = validateBenchmarkPackage(packageValue).join("\n");
  assert.match(errors, /text qualifying rate/);
  assert.match(errors, /aggregate/);
});
