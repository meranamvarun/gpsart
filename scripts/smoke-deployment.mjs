import process from "node:process";

const origin = process.argv[2];
if (!origin || !/^https:\/\/[a-z0-9.-]+$/.test(origin)) {
  throw new Error("usage: node scripts/smoke-deployment.mjs https://canonical-host");
}

async function checked(path, options = {}) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);
  try {
    const response = await fetch(`${origin}${path}`, { ...options, redirect: "error", signal: controller.signal });
    if (!response.ok) throw new Error(`${path} returned ${response.status}`);
    return response;
  } finally {
    clearTimeout(timer);
  }
}

const home = await checked("/");
for (const header of ["strict-transport-security", "content-security-policy", "x-content-type-options"]) {
  if (!home.headers.has(header)) throw new Error(`edge response is missing ${header}`);
}
await checked("/health/live");
await checked("/health/ready");
const designs = await checked("/api/v1/designs");
const payload = await designs.json();
if (!Array.isArray(payload.designs)) throw new Error("design catalogue response is malformed");
const rejectedController = new AbortController();
const rejectedTimer = setTimeout(() => rejectedController.abort(), 10_000);
const rejectedOrigin = await fetch(`${origin}/api/v1/guest-sessions`, {
  method: "POST",
  headers: { Origin: "https://cross-origin.invalid", "Content-Type": "application/json" },
  body: "{}",
  redirect: "error",
  signal: rejectedController.signal,
}).finally(() => clearTimeout(rejectedTimer));
if (rejectedOrigin.status !== 403) {
  throw new Error(`cross-origin session request returned ${rejectedOrigin.status}`);
}
const session = await checked("/api/v1/guest-sessions", {
  method: "POST",
  headers: { Origin: origin, "Content-Type": "application/json" },
  body: "{}",
});
const cookies = session.headers.getSetCookie?.() ?? [];
if (!cookies.some((cookie) => cookie.startsWith("gpsart_guest=") && /;\s*Secure/i.test(cookie))) {
  throw new Error("connected smoke did not receive the Secure guest cookie");
}
console.log("deployment smoke passed");
