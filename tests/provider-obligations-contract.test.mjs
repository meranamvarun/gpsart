import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");
const json = async (path) => JSON.parse(await source(path));

test("provider review binds source configuration and fails closed without a deployed capture", async () => {
  const [review, production, routing] = await Promise.all([
    json("../ops/providers/production-review.json"),
    json("../deploy/environments/production.json"),
    json("../ops/routing/production-pilot-v1.json"),
  ]);

  assert.equal(review.schema_version, "gpsart-provider-review-v1");
  assert.equal(review.environment, "production");
  assert.equal(review.review_status, "blocked_pending_protected_evidence");
  assert.equal(review.origin.hostname_variable, production.hostnameVariable);
  assert.equal(review.origin.capture_required, true);
  assert.equal(review.origin.capture_status, "pending_protected_deployment");
  assert.equal(review.origin.capture_artifact, "release/provider-production-capture.json");
  assert.ok(review.release_blockers.includes("deployed_origin_capture"));
  assert.ok(review.release_blockers.includes("accountable_provider_approval"));
  assert.ok(review.release_blockers.includes("accountable_legal_approval"));
  assert.ok(review.release_blockers.includes("budget_approval"));

  assert.equal(review.providers.length, 2);
  const tiles = review.providers.find(({ id }) => id === "osm-standard-raster");
  assert.equal(tiles.endpoint, "https://tile.openstreetmap.org/{z}/{x}/{y}.png");
  assert.match(tiles.attribution.html, /openstreetmap\.org\/copyright/);
  assert.equal(tiles.credentials.classification, "none");
  assert.equal(tiles.origin_behavior.request_path, "browser_direct");
  assert.equal(tiles.origin_behavior.referrer_policy_source, "deploy/edge/nginx.conf");
  assert.equal(tiles.origin_behavior.deployed_observation, "pending");
  assert.equal(tiles.contact_channel.status, "pending_production_origin");
  assert.equal(tiles.user_agent, "browser_supplied");
  assert.equal(tiles.cache.configured_behavior, "browser_http_cache");
  assert.equal(tiles.cache.deployed_observation, "pending");
  assert.equal(tiles.availability, "best_effort_no_sla");

  const routingProvider = review.providers.find(({ id }) => id === "pinned-local-osm-routing");
  assert.equal(routingProvider.endpoint, "local_checksum_pinned_graph");
  assert.equal(routingProvider.credentials.classification, "none");
  assert.equal(routingProvider.source.version, routing.map_data_version);
  assert.equal(routingProvider.source.sha256, routing.graph.sha256);
  assert.equal(routingProvider.attribution.text, routing.graph.attribution);
});

test("capacity assumptions are reproducible and do not invent an OSM quota or approval", async () => {
  const { capacity } = await json("../ops/providers/production-review.json");

  assert.equal(capacity.policy_decision, "pending_accountable_review");
  assert.equal(capacity.remote_provider_load_test_performed, false);
  assert.match(capacity.remote_provider_load_test_reason, /must not (?:be )?load-tested/i);
  assert.equal(capacity.published_numeric_quota, null);
  for (const tier of [capacity.pilot, capacity.planning_growth]) {
    assert.equal(tier.status, "unapproved_planning_assumption");
    assert.ok(tier.monthly_active_users > 0);
    assert.ok(tier.map_views_per_user > 0);
    assert.ok(tier.tiles_per_view > 0);
    assert.equal(
      tier.expected_monthly_tile_requests,
      tier.monthly_active_users * tier.map_views_per_user * tier.tiles_per_view,
    );
  }
  assert.equal(capacity.guardrail.release_without_accountable_review, "blocked");
  assert.equal(
    capacity.guardrail.action_if_unsuitable,
    "disable_public_tiles_until_paid_hosted_or_self_hosted_replacement_is_approved",
  );
  assert.equal(capacity.guardrail.automatic_provider_substitution, false);
  assert.ok(capacity.local_concurrency_test.jobs >= 20);
  assert.ok(capacity.local_concurrency_test.evidence.every((path) => path.startsWith("docs/evidence/") || path.startsWith("tests/")));
});

test("provider failures preserve user state and never substitute routing mode", async () => {
  const [review, failureEvidence, journey, outcomeFlow] = await Promise.all([
    json("../ops/providers/production-review.json"),
    json("../docs/evidence/ts-403-failure-injection.json"),
    source("../tests/route-candidate-journey.test.ts"),
    source("../src/app/outcome-flow.ts"),
  ]);
  for (const scenario of ["base_map_slow_or_absent", "provider_timeout", "provider_quota"]) {
    assert.equal(failureEvidence.scenarios[scenario].passed, true);
  }
  for (const failure of ["outage", "throttling", "quota", "slow_response"]) {
    assert.equal(review.failure_behavior[failure].preserves_form_state, true);
    assert.equal(review.failure_behavior[failure].preserves_generated_route_data, true);
    assert.equal(review.failure_behavior[failure].automatic_routing_mode_substitution, false);
    assert.match(review.failure_behavior[failure].public_state, /unavailable|delayed/);
  }
  assert.match(journey, /tile-error handler/i);
  assert.match(outcomeFlow, /provider_unavailable/);
});

test("licensing record covers source, derived data, exports, notices, and publication gates", async () => {
  const { licensing } = await json("../ops/providers/production-review.json");

  assert.equal(licensing.source.license, "ODbL-1.0");
  assert.equal(licensing.derived_routing_database.working_classification, "derivative_database");
  assert.equal(licensing.derived_routing_database.current_distribution, "embedded_in_unreleased_production_service");
  assert.equal(licensing.derived_routing_database.public_use_offer, "blocked_pending_legal_review");
  assert.equal(licensing.route_exports.working_classification, "produced_work_pending_legal_review");
  assert.equal(licensing.route_exports.attribution_required, true);
  assert.equal(licensing.route_exports.public_display_or_export, "blocked_pending_odbl_4_6_review_and_offer");
  assert.equal(licensing.notices.in_product, true);
  assert.equal(licensing.notices.gpx.current, "attribution_text_only");
  assert.equal(licensing.notices.gpx.odbl_link_present, false);
  assert.equal(licensing.notices.gpx.adequacy, "pending_accountable_legal_review");
  assert.equal(licensing.publication_plan.current_status, "blocked");
  assert.equal(
    licensing.publication_plan.release_gate,
    "accountable_legal_review_and_odbl_4_6_offer_required_before_public_route_display_or_export",
  );
  assert.ok(licensing.sources.every(({ url, reviewed_at }) => url.startsWith("https://") && /^\d{4}-\d{2}-\d{2}$/.test(reviewed_at)));
});

test("monthly cost estimates are reproducible but cannot masquerade as approved budget", async () => {
  const model = await json("../ops/providers/monthly-cost-model.json");

  assert.equal(model.schema_version, "gpsart-monthly-cost-model-v1");
  assert.equal(model.currency, "USD");
  assert.equal(model.estimate_status, "planning_estimate_pending_vendor_quotes_and_approval");
  const required = ["compute", "storage", "backup", "bandwidth", "tile_provider", "monitoring", "support"];
  for (const tierName of ["pilot", "planning_growth"]) {
    const tier = model.tiers[tierName];
    assert.equal(tier.status, "unapproved_planning_assumption");
    assert.deepEqual(tier.categories.map(({ id }) => id), required);
    assert.ok(tier.categories.every(({ quantity, unit_rate, monthly_estimate, basis }) =>
      quantity >= 0 && unit_rate >= 0 && monthly_estimate === quantity * unit_rate && basis.length > 0));
    assert.equal(tier.monthly_total, tier.categories.reduce((sum, item) => sum + item.monthly_estimate, 0));
    assert.ok(tier.capacity.monthly_active_users > 0);
    assert.ok(tier.capacity.monthly_tile_requests > 0);
  }
  assert.ok(model.tiers.planning_growth.monthly_total >= model.tiers.pilot.monthly_total);
  assert.ok(model.assumptions.every(({ owner_role, status, review_trigger }) =>
    owner_role && status === "unapproved" && review_trigger));
  assert.deepEqual(model.approvals.map(({ role }) => role).sort(), ["engineering-lead", "finance-owner", "platform-owner"]);
  assert.ok(model.approvals.every(({ decision, recorded_at, blocker }) =>
    decision === "pending" && recorded_at === null && blocker.length > 0));
  assert.equal(model.release_gate, "blocked_pending_budget_approval");
});

test("capture records allowlisted edge observations without an automated tile request", async () => {
  const { captureProviderReview } = await import("../scripts/capture-provider-review.mjs");
  const requests = [];
  const fakeFetch = async (url, init = {}) => {
    requests.push({ url: String(url), headers: { ...(init.headers ?? {}) }, signal: init.signal });
    return new Response("ok", {
      status: 200,
      headers: {
        "content-security-policy": "default-src 'self'; img-src https://tile.openstreetmap.org",
        "referrer-policy": "strict-origin-when-cross-origin",
      },
    });
  };

  const capture = await captureProviderReview({
    origin: "https://prod.gpsart.test",
    fetchImpl: fakeFetch,
    capturedAt: "2026-10-20T00:00:00.000Z",
    sourceRevision: "0123456789abcdef0123456789abcdef01234567",
  });
  assert.equal(capture.schema_version, "gpsart-provider-capture-v1");
  assert.equal(capture.result, "edge_capture_passed_provider_observation_pending");
  assert.equal(capture.source_revision, "0123456789abcdef0123456789abcdef01234567");
  assert.equal(capture.origin.referrer_policy, "strict-origin-when-cross-origin");
  assert.equal(capture.provider.request_performed, false);
  assert.equal(capture.provider.observation_status, "pending_human_browser_capture");
  assert.equal(requests.length, 1);
  assert.equal(requests[0].url, "https://prod.gpsart.test/");
  assert.ok(requests.every(({ signal }) => signal instanceof AbortSignal));
  const serialized = JSON.stringify(capture).toLowerCase();
  for (const forbidden of ["cookie", "authorization", "request_body", "longitude", "latitude", "geometry", "trace"]) {
    assert.doesNotMatch(serialized, new RegExp(forbidden));
  }
});

test("capture fails closed on an incompatible edge policy or non-origin URL", async () => {
  const { captureProviderReview } = await import("../scripts/capture-provider-review.mjs");
  const common = {
    origin: "https://prod.gpsart.test",
    capturedAt: "2026-10-20T00:00:00.000Z",
    sourceRevision: "0123456789abcdef0123456789abcdef01234567",
  };

  await assert.rejects(
    captureProviderReview({
      ...common,
      fetchImpl: async () => new Response("origin", {
        status: 200,
        headers: {
          "content-security-policy": "img-src https://tile.openstreetmap.org",
          "referrer-policy": "no-referrer",
        },
      }),
    }),
    /production_referrer_policy_missing_or_incompatible/,
  );

  await assert.rejects(
    captureProviderReview({
      ...common,
      origin: "https://prod.gpsart.test/private/path",
      fetchImpl: async () => assert.fail("invalid origin must fail before network access"),
    }),
    /invalid_production_origin/,
  );

  await assert.rejects(
    captureProviderReview({
      ...common,
      fetchImpl: async () => new Response("origin", {
        status: 200,
        headers: {
          "content-security-policy": "img-src https://tile.openstreetmap.org, img-src 'none'",
          "referrer-policy": "strict-origin-when-cross-origin",
        },
      }),
    }),
    /production_multiple_csp_policies_unsupported/,
  );
});

test("production workflow captures and retains evidence while checked-in review stays pending", async () => {
  const [delivery, packageJson] = await Promise.all([
    source("../.github/workflows/deliver.yml"),
    json("../package.json"),
  ]);

  assert.match(delivery, /capture-provider-review\.mjs/);
  assert.match(delivery, /provider-production-capture\.json/);
  assert.match(delivery, /if: always\(\)/);
  assert.match(delivery, /actions\/upload-artifact@[a-f0-9]{40}/);
  assert.match(packageJson.scripts["test:provider"], /provider-obligations-contract\.test\.mjs/);
  await access(new URL("../docs/evidence/ts-509-provider-obligations-verification.md", import.meta.url));
});
