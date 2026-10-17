import assert from "node:assert/strict";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import process from "node:process";
import puppeteer from "puppeteer-core";

const root = process.cwd();
const evidence = path.join(root, "docs", "evidence");
const chrome = process.env.GPSART_CHROME_PATH
  ?? "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const port = 4174;
const origin = `http://127.0.0.1:${port}`;
const viewports = [
  { name: "mobile-360", width: 360, height: 800 },
  { name: "tablet-768", width: 768, height: 900 },
  { name: "desktop-1440", width: 1440, height: 900 },
];

const contentTypes = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".png": "image/png", ".jpg": "image/jpeg", ".woff2": "font/woff2" };
const server = createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(new URL(request.url ?? "/", origin).pathname);
    const relative = pathname === "/" ? "index.html" : pathname.replace(/^\//, "");
    const target = path.resolve(root, "dist", "client", relative);
    const distRoot = path.resolve(root, "dist", "client");
    if (!target.startsWith(`${distRoot}${path.sep}`)) throw new Error("invalid path");
    const body = await readFile(target);
    response.writeHead(200, { "content-type": contentTypes[path.extname(target)] ?? "application/octet-stream" });
    response.end(body);
  } catch {
    response.writeHead(404);
    response.end("Not found");
  }
});

let browser;
try {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  await mkdir(evidence, { recursive: true });
  browser = await puppeteer.launch({ executablePath: chrome, headless: true, args: ["--disable-gpu", "--no-proxy-server"] });
  const report = [];

  for (const viewport of viewports) {
    const page = await browser.newPage();
    page.on("pageerror", (error) => console.error(`browser page error: ${error.message}`));
    page.on("requestfailed", (request) => console.error(`browser request failed: ${request.url()} (${request.failure()?.errorText ?? "unknown"})`));
    await page.setViewport({ width: viewport.width, height: viewport.height, deviceScaleFactor: 1 });
    await page.goto(`${origin}/#/planner`, { waitUntil: "domcontentloaded", timeout: 5_000 }).catch(() => undefined);
    await page.waitForSelector("#plan-form", { timeout: 5_000 }).catch(async (error) => {
      throw new Error(`Planner did not render at ${page.url()}: ${(await page.content()).slice(0, 500)}`, { cause: error });
    });
    await page.addScriptTag({ path: path.join(root, "node_modules", "axe-core", "axe.min.js") });

    const audit = await page.evaluate(async () => {
      const axeResult = await globalThis.axe.run(document, {
        runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"] },
      });
      const primary = document.querySelector("#plan-form .primary");
      primary?.scrollIntoView({ block: "center" });
      const rect = primary?.getBoundingClientRect();
      return {
        documentWidth: document.documentElement.scrollWidth,
        viewportWidth: document.documentElement.clientWidth,
        primaryActionVisible: !!rect && rect.left >= 0 && rect.right <= window.innerWidth && rect.top >= 0 && rect.bottom <= window.innerHeight,
        activeNavigationCurrent: document.querySelector('.nav a.active')?.getAttribute("aria-current") === "page",
        violations: axeResult.violations.map(({ id, impact, help, nodes }) => ({
          id, impact, help,
          nodes: nodes.map(({ target, failureSummary }) => ({ target, failureSummary })),
        })),
      };
    });
    let assistiveSmoke = null;
    if (viewport.width === 360) {
      await page.reload({ waitUntil: "domcontentloaded" });
      await page.waitForSelector("#plan-form");
      const publicSurface = await page.evaluate(() => {
        const savedControl = document.querySelector('.nav a[href="#/saved"]');
        const rect = savedControl?.getBoundingClientRect();
        return {
          savedName: savedControl?.textContent?.trim(),
          savedVisible: !!rect && rect.width > 0 && rect.height > 0 && rect.left >= 0 && rect.right <= window.innerWidth,
          hasAuthenticationControl: !!document.querySelector('.signin, .google, [href*="login"], [data-action="google"]'),
          hasPlaceSearchControl: !!document.querySelector('[type="search"], [data-action*="search"], [aria-label*="place" i]'),
        };
      });
      assert.deepEqual(publicSurface, {
        savedName: "Saved",
        savedVisible: true,
        hasAuthenticationControl: false,
        hasPlaceSearchControl: false,
      }, "mobile public navigation must expose guest saves without unsupported sign-in or place search");

      const focusOrder = [];
      for (let index = 0; index < 14; index += 1) {
        await page.keyboard.press("Tab");
        focusOrder.push(await page.evaluate(() => {
          const active = document.activeElement;
          return active instanceof HTMLElement
            ? active.getAttribute("aria-label") ?? active.textContent?.trim().replace(/\s+/g, " ") ?? active.id
            : "";
        }));
      }
      assert.equal(focusOrder[0], "Skip to content", "skip link must be first in keyboard focus order");
      assert.ok(focusOrder.includes("Saved"), "mobile device-only saves must be keyboard reachable");
      assert.ok(focusOrder.some((name) => name.includes("Use my location")), "planner controls must be reachable by keyboard");

      await page.focus("#design-tab-shapes");
      await page.keyboard.press("ArrowRight");
      const tabKeyboard = await page.evaluate(() => ({
        activeId: document.activeElement?.id,
        selectedText: document.querySelector("#design-tab-text")?.getAttribute("aria-selected"),
        outlineWidth: getComputedStyle(document.activeElement).outlineWidth,
      }));
      assert.deepEqual(tabKeyboard, { activeId: "design-tab-text", selectedText: "true", outlineWidth: "3px" });
      await page.keyboard.press("ArrowLeft");

      const client = await page.createCDPSession();
      await client.send("Accessibility.enable");
      const tree = await client.send("Accessibility.getFullAXTree");
      const accessibleNodes = tree.nodes.flatMap((node) => node.role?.value && node.name?.value
        ? [{ role: node.role.value, name: node.name.value }]
        : []);
      assert.ok(accessibleNodes.some(({ role, name }) => role === "textbox" && name === "Starting point"));
      assert.ok(accessibleNodes.some(({ role, name }) => role === "tab" && name === "Shapes"));
      assert.ok(accessibleNodes.some(({ role, name }) => role === "button" && name.includes("Walking")));
      assistiveSmoke = { focusOrder, tabKeyboard, accessibleNodes: accessibleNodes.filter(({ role }) => ["textbox", "tab", "button", "navigation", "main"].includes(role)) };
    }
    await page.evaluate(() => {
      if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
      document.querySelectorAll("*").forEach((element) => { element.scrollTop = 0; element.scrollLeft = 0; });
      window.scrollTo(0, 0);
    });
    await page.screenshot({ path: path.join(evidence, `ts-504-public-surface-${viewport.name}.png`), fullPage: true });

    assert.ok(audit.documentWidth <= audit.viewportWidth, `${viewport.name} has page-level horizontal overflow (${audit.documentWidth} > ${audit.viewportWidth})`);
    assert.equal(audit.primaryActionVisible, true, `${viewport.name} primary action cannot be brought into view`);
    assert.equal(audit.activeNavigationCurrent, true, `${viewport.name} active navigation is expressed only by colour`);
    const blocking = audit.violations.filter(({ impact }) => impact === "serious" || impact === "critical");
    assert.deepEqual(blocking, [], `${viewport.name} has serious or critical axe findings`);
    report.push({ viewport, ...audit, blockingViolations: blocking.length, assistiveSmoke });
    await page.close();
  }

  for (const surface of ["catalogue", "saved", "results"]) {
    const page = await browser.newPage();
    await page.setViewport({ width: 360, height: 800, deviceScaleFactor: 1 });
    await page.goto(`${origin}/#/${surface}`, { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#content");
    await page.addScriptTag({ path: path.join(root, "node_modules", "axe-core", "axe.min.js") });
    const audit = await page.evaluate(async () => {
      const axeResult = await globalThis.axe.run(document, {
        runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"] },
      });
      return {
        documentWidth: document.documentElement.scrollWidth,
        viewportWidth: document.documentElement.clientWidth,
        overflowingElements: [...document.querySelectorAll("body *")].flatMap((element) => {
          const rect = element.getBoundingClientRect();
          return rect.left < -0.5 || rect.right > window.innerWidth + 0.5
            ? [{ element: `${element.tagName.toLowerCase()}${element.id ? `#${element.id}` : ""}${element.className && typeof element.className === "string" ? `.${element.className.trim().replace(/\s+/g, ".")}` : ""}`, left: rect.left, right: rect.right }]
            : [];
        }).slice(0, 12),
        violations: axeResult.violations.map(({ id, impact, help, nodes }) => ({
          id, impact, help,
          nodes: nodes.map(({ target, failureSummary }) => ({ target, failureSummary })),
        })),
      };
    });
    assert.ok(audit.documentWidth <= audit.viewportWidth, `${surface} has page-level horizontal overflow (${audit.documentWidth} > ${audit.viewportWidth}): ${JSON.stringify(audit.overflowingElements)}`);
    const blocking = audit.violations.filter(({ impact }) => impact === "serious" || impact === "critical");
    assert.deepEqual(blocking, [], `${surface} has serious or critical axe findings`);
    report.push({ surface, viewport: { name: "mobile-360", width: 360, height: 800 }, ...audit, blockingViolations: blocking.length });
    await page.close();
  }

  for (const staleRoute of ["login", "auth-pending", "auth-error", "welcome", "account"]) {
    const page = await browser.newPage();
    await page.goto(`${origin}/#/${staleRoute}?enabled=true`, { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#plan-form");
    assert.equal(await page.evaluate(() => window.location.hash), `#/${staleRoute}?enabled=true`);
    assert.equal(await page.evaluate(() => document.querySelector("#plan-form") !== null), true, `${staleRoute} must fall back to the guest planner`);
    assert.equal(await page.evaluate(() => document.querySelector('.signin, .google, [data-action="google"]') === null), true);
    await page.close();
  }

  await writeFile(path.join(evidence, "ts-504-public-surface-report.json"), `${JSON.stringify({ standard: "axe-core WCAG 2 A/AA, 2.1 AA, 2.2 AA", report, staleAuthenticationRoutes: "guest planner fallback" }, null, 2)}\n`);
  console.log(`Accessibility and TS-504 public-surface audit passed for planner widths ${viewports.map(({ width }) => `${width}px`).join(", ")} and catalogue/saved/results surfaces; no serious or critical axe findings.`);
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
