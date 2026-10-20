import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const TILE_ENDPOINT = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
const REVISION = /^[a-f0-9]{40}$/;
const BROWSER_FAMILIES = new Set(["Chrome", "Firefox", "Safari", "Mobile Chrome", "Mobile Safari"]);
const BROWSER_VERSION = /^\d{1,3}(?:\.\d{1,4}){0,3}$/;

class BrowserCaptureError extends Error {
  constructor(message) {
    super(message);
    this.name = "BrowserCaptureError";
  }
}

function record(value, path) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new BrowserCaptureError(`${path} must be an object`);
  }
  return value;
}

function exactKeys(value, expected, path) {
  const actual = Object.keys(record(value, path)).sort();
  const wanted = [...expected].sort();
  const unexpected = actual.filter((key) => !wanted.includes(key));
  const missing = wanted.filter((key) => !actual.includes(key));
  if (unexpected.length > 0) throw new BrowserCaptureError(`${path} has unexpected field`);
  if (missing.length > 0) throw new BrowserCaptureError(`${path} is missing required field`);
}

function httpsOrigin(value, path) {
  let parsed;
  try {
    parsed = new URL(value);
  } catch {
    throw new BrowserCaptureError(`${path} must be an HTTPS origin`);
  }
  if (parsed.protocol !== "https:" || parsed.username || parsed.password || parsed.pathname !== "/" || parsed.search || parsed.hash) {
    throw new BrowserCaptureError(`${path} must be an HTTPS origin`);
  }
  return parsed.origin;
}

function canonicalInstant(value) {
  if (typeof value !== "string") throw new BrowserCaptureError("captured_at must be a canonical instant");
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime()) || parsed.toISOString() !== value) {
    throw new BrowserCaptureError("captured_at must be a canonical instant");
  }
  return value;
}

function nullableHeader(value, path) {
  if (value === null) return null;
  if (typeof value !== "string" || value.length < 1 || value.length > 512 || /[\r\n]/.test(value)) {
    throw new BrowserCaptureError(`${path} must be a bounded single-line header`);
  }
  return value;
}

function validateCacheBehavior(response, capturedAt) {
  const cacheControl = nullableHeader(response.cache_control, "provider.response.cache_control");
  const expires = nullableHeader(response.expires, "provider.response.expires");
  if (cacheControl === null && expires === null) {
    throw new BrowserCaptureError("provider cache behavior is missing");
  }
  if (/\b(?:no-cache|no-store|max-age\s*=\s*0)\b/i.test(cacheControl ?? "")) {
    throw new BrowserCaptureError("provider cache behavior disables caching");
  }
  const maxAge = /(?:^|,)\s*max-age\s*=\s*(\d+)\b/i.exec(cacheControl ?? "");
  const hasPositiveMaxAge = maxAge !== null && Number(maxAge[1]) > 0;
  let hasFutureExpiry = false;
  if (expires !== null) {
    const expiryDate = new Date(expires);
    if (Number.isNaN(expiryDate.getTime()) || expiryDate.toUTCString() !== expires) {
      throw new BrowserCaptureError("provider cache Expires is not a canonical HTTP date");
    }
    if (expiryDate.getTime() <= Date.parse(capturedAt)) {
      throw new BrowserCaptureError("provider cache behavior has no future expiry");
    }
    hasFutureExpiry = true;
  }
  if (!hasPositiveMaxAge && !hasFutureExpiry) throw new BrowserCaptureError("provider cache behavior has no positive lifetime");
  return { cacheControl, expires };
}

export function verifyProviderBrowserCapture(value, { expectedOrigin, expectedSourceRevision }) {
  if (!REVISION.test(expectedSourceRevision ?? "")) throw new BrowserCaptureError("expected source revision is invalid");
  const origin = httpsOrigin(expectedOrigin, "expected origin");

  exactKeys(value, ["schema_version", "source_revision", "captured_at", "capture_method", "browser", "origin", "provider", "privacy"], "capture");
  if (value.schema_version !== "gpsart-human-browser-provider-capture-v1") throw new BrowserCaptureError("capture schema version is unsupported");
  if (value.source_revision !== expectedSourceRevision) throw new BrowserCaptureError("capture source revision does not match");
  const capturedAt = canonicalInstant(value.captured_at);
  if (value.capture_method !== "human_browser_developer_tools") throw new BrowserCaptureError("capture method must be human browser developer tools");
  if (httpsOrigin(value.origin, "capture origin") !== origin) throw new BrowserCaptureError("capture origin does not match");

  exactKeys(value.browser, ["family", "version"], "browser");
  if (!BROWSER_FAMILIES.has(value.browser.family)) throw new BrowserCaptureError("browser family is unsupported");
  if (typeof value.browser.version !== "string" || !BROWSER_VERSION.test(value.browser.version)) {
    throw new BrowserCaptureError("browser version is invalid");
  }
  const browserFamily = value.browser.family;
  const browserVersion = value.browser.version;

  exactKeys(value.provider, ["id", "endpoint_template", "request", "response"], "provider");
  if (value.provider.id !== "osm-standard-raster" || value.provider.endpoint_template !== TILE_ENDPOINT) {
    throw new BrowserCaptureError("provider identity or endpoint does not match");
  }
  exactKeys(value.provider.request, ["referrer_origin", "user_agent_class", "cache_bypass_headers_sent"], "provider.request");
  if (httpsOrigin(value.provider.request.referrer_origin, "provider request referrer origin") !== origin) {
    throw new BrowserCaptureError("provider request referrer origin does not match");
  }
  if (value.provider.request.user_agent_class !== "browser_supplied" || value.provider.request.cache_bypass_headers_sent !== false) {
    throw new BrowserCaptureError("provider request identification or caching behavior is invalid");
  }

  exactKeys(value.provider.response, ["status", "content_type", "cache_control", "expires"], "provider.response");
  if (value.provider.response.status !== 200) {
    throw new BrowserCaptureError("provider response status must be 200");
  }
  if (value.provider.response.content_type !== "image/png") throw new BrowserCaptureError("provider response content type is not image/png");
  const cache = validateCacheBehavior(value.provider.response, capturedAt);

  exactKeys(value.privacy, ["response_body_recorded", "exact_tile_url_recorded", "precise_route_data_recorded", "session_data_recorded", "request_headers_recorded"], "privacy");
  for (const key of ["response_body_recorded", "exact_tile_url_recorded", "precise_route_data_recorded", "session_data_recorded"]) {
    if (value.privacy[key] !== false) throw new BrowserCaptureError("capture privacy boundary was not preserved");
  }
  if (!Array.isArray(value.privacy.request_headers_recorded)
    || value.privacy.request_headers_recorded.length !== 1
    || value.privacy.request_headers_recorded[0] !== "referer") {
    throw new BrowserCaptureError("capture request-header allowlist is invalid");
  }

  return {
    schema_version: "gpsart-human-browser-provider-verification-v1",
    result: "schema_validated",
    source_revision: expectedSourceRevision,
    captured_at: capturedAt,
    origin,
    browser: { family: browserFamily, version: browserVersion },
    provider: {
      id: "osm-standard-raster",
      referrer_origin_matches: true,
      content_type: "image/png",
      cache_behavior_observed: true,
      cache_control_present: cache.cacheControl !== null,
      expires_present: cache.expires !== null,
    },
    privacy_boundary: "preserved_by_schema",
  };
}

async function main() {
  const [input, expectedOrigin, expectedSourceRevision, output] = process.argv.slice(2);
  if (!input || !expectedOrigin || !expectedSourceRevision || !output || process.argv.length !== 6) {
    throw new BrowserCaptureError("usage: input expected-origin expected-source-revision output");
  }
  const capture = JSON.parse(await readFile(resolve(input), "utf8"));
  const verified = verifyProviderBrowserCapture(capture, { expectedOrigin, expectedSourceRevision });
  await writeFile(resolve(output), `${JSON.stringify(verified, null, 2)}\n`, { encoding: "utf8", mode: 0o600 });
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  main().catch((error) => {
    const message = error instanceof BrowserCaptureError ? error.message : "browser capture verification failed";
    process.stderr.write(`Provider browser capture rejected: ${message}\n`);
    process.exitCode = 1;
  });
}
