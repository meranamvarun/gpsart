import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, resolve } from "node:path";
import test from "node:test";

import puppeteer from "puppeteer-core";

const chrome = [
  process.env.CHROME_PATH,
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
].find((candidate) => candidate && existsSync(candidate));

const contentTypes = {
  ".css": "text/css",
  ".html": "text/html",
  ".js": "text/javascript",
  ".svg": "image/svg+xml",
  ".woff2": "font/woff2",
};

function json(response, status, payload, headers = {}) {
  response.writeHead(status, { "content-type": "application/json", ...headers });
  response.end(JSON.stringify(payload));
}

async function fixtureServer() {
  const dist = resolve("dist", "client");
  const calls = { resolutions: 0, jobs: 0, startRequests: [], rejectNextResolutionCode: null };
  const server = createServer(async (request, response) => {
    const url = new URL(request.url ?? "/", "http://localhost");
    if (url.pathname === "/api/v1/guest-sessions") {
      json(response, 201, { expires_at: 2_000_000_000, rotate_at: 1_999_956_800, rotate_after: 43_200 }, {
        "set-cookie": "gpsart_csrf=browser-test; Path=/; SameSite=Strict",
      });
      return;
    }
    if (url.pathname === "/api/v1/designs") {
      json(response, 200, { designs: [{
        id: "heart", version: 1, status: "published", category: "shape", name: "Heart",
        accessible_name: "Heart route design", complexity_hint: "simple", preview_svg: "<svg></svg>", definition_hash: "a".repeat(64),
      }] });
      return;
    }
    if (url.pathname === "/api/v1/starts/resolve") {
      calls.resolutions += 1;
      calls.startRequests.push({
        method: request.method,
        authorized: request.headers["x-csrf-token"] === "browser-test"
          && request.headers.cookie?.includes("gpsart_csrf=browser-test") === true
          && request.headers.origin === `http://${request.headers.host}`,
      });
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      await new Promise((done) => setTimeout(done, 75));
      if (calls.rejectNextResolutionCode) {
        const code = calls.rejectNextResolutionCode;
        calls.rejectNextResolutionCode = null;
        json(response, code === "authentication_required" ? 401 : 403, { code });
        return;
      }
      json(response, 200, {
        id: "7bf80ccf-68e8-49ec-a3e8-cad667bb95a0",
        requested_point: body.requested_point,
        candidate_network_point: body.requested_point,
        snap_distance_metres: 0,
        profile_version: "walking-v1",
        coverage_policy_version: "coverage-v1",
        data_version: "map-v1",
        expires_at_epoch_seconds: 1_999_999_999,
        confirmation_required: true,
      });
      return;
    }
    if (url.pathname === "/api/v1/route-jobs") {
      calls.jobs += 1;
      json(response, 500, {});
      return;
    }
    const relative = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
    const file = join(dist, relative);
    if (!file.startsWith(dist)) {
      response.writeHead(404).end();
      return;
    }
    try {
      const body = await readFile(file);
      response.writeHead(200, { "content-type": contentTypes[extname(file)] ?? "application/octet-stream" });
      response.end(body);
    } catch {
      response.writeHead(404).end();
    }
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  const address = server.address();
  assert.ok(address && typeof address === "object");
  return { server, calls, origin: `http://127.0.0.1:${address.port}` };
}

async function readyPage(browser, origin) {
  const page = await browser.newPage();
  await page.goto(`${origin}/#/planner`, { waitUntil: "domcontentloaded" });
  await page.waitForSelector('button[type="submit"]:not([disabled])');
  return page;
}

test("built planner resolves manual and map starts once and stops for confirmation", { timeout: 30_000 }, async () => {
  assert.ok(chrome, "set CHROME_PATH or install Chrome/Chromium to run the planner browser regression");
  const fixture = await fixtureServer();
  const browser = await puppeteer.launch({ executablePath: chrome, headless: true, args: ["--disable-gpu", "--no-proxy-server"] });
  try {
    const manual = await readyPage(browser, fixture.origin);
    await manual.$eval("#start", (element) => {
      element.value = "0.50000, -150.25000";
      element.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await manual.$eval("#plan-form", (form) => { form.requestSubmit(); form.requestSubmit(); });
    await manual.waitForSelector('[data-action="confirm-start"]');
    assert.equal(fixture.calls.resolutions, 1);
    assert.deepEqual(fixture.calls.startRequests, [{ method: "POST", authorized: true }]);
    assert.equal(fixture.calls.jobs, 0);
    await manual.close();

    const map = await readyPage(browser, fixture.origin);
    const bounds = await map.$eval("#planner-map", (element) => {
      const rect = element.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    });
    await map.mouse.click(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
    await map.waitForFunction(() => document.querySelector("#start")?.value.includes(","));
    await map.$eval("#plan-form", (form) => { form.requestSubmit(); form.requestSubmit(); });
    await map.waitForSelector('[data-action="confirm-start"]');
    assert.equal(fixture.calls.resolutions, 2);
    assert.deepEqual(fixture.calls.startRequests, [
      { method: "POST", authorized: true },
      { method: "POST", authorized: true },
    ]);
    assert.equal(fixture.calls.jobs, 0);
    await map.close();

    const expiredSession = await readyPage(browser, fixture.origin);
    fixture.calls.rejectNextResolutionCode = "authentication_required";
    await expiredSession.$eval("#start", (element) => {
      element.value = "0.50000, -150.25000";
      element.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await expiredSession.click('button[type="submit"]');
    await expiredSession.waitForFunction(() => document.body.innerText.includes("private guest session is not ready"));
    assert.equal(fixture.calls.resolutions, 3);
    assert.equal(fixture.calls.startRequests.at(-1)?.authorized, true);
    assert.equal(fixture.calls.jobs, 0);
    await expiredSession.close();

    const missingSession = await readyPage(browser, fixture.origin);
    const before = fixture.calls.resolutions;
    await missingSession.deleteCookie({ name: "gpsart_csrf", url: fixture.origin });
    await missingSession.$eval("#start", (element) => {
      element.value = "0.50000, -150.25000";
      element.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await missingSession.click('button[type="submit"]');
    await missingSession.waitForFunction(() => document.body.innerText.includes("private guest session is not ready"));
    assert.equal(fixture.calls.resolutions, before);
  } finally {
    await browser.close();
    serverCloseConnections(fixture.server);
    await new Promise((done) => fixture.server.close(done));
  }
});

function serverCloseConnections(server) {
  server.closeAllConnections?.();
}
