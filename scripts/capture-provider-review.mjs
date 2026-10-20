import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const TILE_TEMPLATE = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
const REQUIRED_REFERRER_POLICY = "strict-origin-when-cross-origin";

class CaptureFailure extends Error {
  constructor(code) {
    super(code);
    this.name = "CaptureFailure";
    this.code = code;
  }
}

function normalizedOrigin(value) {
  const parsed = new URL(value);
  if (parsed.protocol !== "https:" || parsed.username || parsed.password || parsed.pathname !== "/" || parsed.search || parsed.hash) {
    throw new CaptureFailure("invalid_production_origin");
  }
  return parsed.origin;
}

function assertRevision(value) {
  if (!/^[a-f0-9]{40}$/.test(value)) throw new CaptureFailure("invalid_source_revision");
  return value;
}

async function discardBody(response) {
  await response.body?.cancel();
}

export async function captureProviderReview({
  origin,
  sourceRevision,
  fetchImpl = fetch,
  capturedAt = new Date().toISOString(),
  timeoutMs = 15_000,
}) {
  const productionOrigin = normalizedOrigin(origin);
  const revision = assertRevision(sourceRevision);

  const originResponse = await fetchImpl(`${productionOrigin}/`, {
    method: "GET",
    redirect: "error",
    signal: AbortSignal.timeout(timeoutMs),
    headers: { accept: "text/html" },
  });
  await discardBody(originResponse);
  if (!originResponse.ok) throw new CaptureFailure("production_origin_unavailable");

  const referrerPolicy = originResponse.headers.get("referrer-policy")?.trim().toLowerCase() ?? null;
  if (referrerPolicy !== REQUIRED_REFERRER_POLICY) {
    throw new CaptureFailure("production_referrer_policy_missing_or_incompatible");
  }
  const contentSecurityPolicy = originResponse.headers.get("content-security-policy") ?? "";
  if (contentSecurityPolicy.includes(",")) {
    throw new CaptureFailure("production_multiple_csp_policies_unsupported");
  }
  const imageSources = contentSecurityPolicy
    .split(";")
    .map((directive) => directive.trim().split(/\s+/))
    .find(([name]) => name.toLowerCase() === "img-src") ?? [];
  if (!imageSources.includes("https://tile.openstreetmap.org")) {
    throw new CaptureFailure("production_tile_source_not_allowed");
  }

  return {
    schema_version: "gpsart-provider-capture-v1",
    captured_at: capturedAt,
    source_revision: revision,
    result: "edge_capture_passed_provider_observation_pending",
    origin: {
      scheme_and_host: productionOrigin,
      status: originResponse.status,
      referrer_policy: referrerPolicy,
      content_security_policy_allows_osm_tiles: true,
    },
    provider: {
      id: "osm-standard-raster",
      endpoint_template: TILE_TEMPLATE,
      request_performed: false,
      observation_status: "pending_human_browser_capture",
      policy_reason: "automated_preemptive_tile_requests_are_not_performed",
      required_human_capture_fields: [
        "request_referrer_origin",
        "response_status",
        "response_cache_control_or_expires",
      ],
    },
    excluded_data: [
      "request_headers_other_than_identification_classes",
      "response_bodies",
      "session_identifiers",
      "precise_route_data",
    ],
  };
}

async function writeCapture(path, value) {
  const output = resolve(path);
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(value, null, 2)}\n`, { encoding: "utf8", mode: 0o600 });
}

async function main() {
  const [origin, output, sourceRevision] = process.argv.slice(2);
  if (!origin || !output || !sourceRevision || process.argv.length !== 5) {
    throw new CaptureFailure("usage_origin_output_source_revision_required");
  }

  try {
    const capture = await captureProviderReview({ origin, sourceRevision });
    await writeCapture(output, capture);
  } catch (error) {
    const failureCode = error instanceof CaptureFailure ? error.code : "network_or_runtime_failure";
    let safeOrigin = null;
    try {
      safeOrigin = normalizedOrigin(origin);
    } catch {
      // The invalid value is intentionally omitted from the artifact.
    }
    await writeCapture(output, {
      schema_version: "gpsart-provider-capture-v1",
      captured_at: new Date().toISOString(),
      source_revision: /^[a-f0-9]{40}$/.test(sourceRevision ?? "") ? sourceRevision : null,
      result: "failed",
      failure_class: failureCode,
      origin: safeOrigin === null ? null : { scheme_and_host: safeOrigin },
    });
    throw new CaptureFailure(failureCode);
  }
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    const code = error instanceof CaptureFailure ? error.code : "network_or_runtime_failure";
    process.stderr.write(`Provider capture failed: ${code}\n`);
    process.exitCode = 1;
  });
}
