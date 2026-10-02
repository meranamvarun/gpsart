import { writeFile } from "node:fs/promises";

const [url, output, widthText, heightText, portText = "9222"] = process.argv.slice(2);
if (!url || !output) throw new Error("usage: node capture-browser.mjs <url> <output> [width] [height] [debug-port]");
const width = Number(widthText ?? 1440);
const height = Number(heightText ?? 1000);
const endpoint = `http://127.0.0.1:${Number(portText)}`;

async function pageTarget() {
  for (let attempt = 0; attempt < 40; attempt += 1) {
    try {
      const targets = await fetch(`${endpoint}/json`).then((response) => response.json());
      const page = targets.find((target) => target.type === "page");
      if (page) return page;
    } catch { /* Browser startup is asynchronous. */ }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("Chrome DevTools endpoint did not become ready");
}

const target = await pageTarget();
const socket = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", reject, { once: true });
});
let sequence = 0;
const pending = new Map();
socket.addEventListener("message", ({ data }) => {
  const message = JSON.parse(String(data));
  if (!message.id) return;
  const callback = pending.get(message.id);
  if (callback) { pending.delete(message.id); callback(message); }
});
function command(method, params = {}) {
  const id = ++sequence;
  socket.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve, reject) => pending.set(id, (message) => message.error ? reject(new Error(message.error.message)) : resolve(message.result)));
}

await command("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: width <= 700 });
await command("Page.navigate", { url });
await new Promise((resolve) => setTimeout(resolve, 750));
const capture = await command("Page.captureScreenshot", { format: "png", fromSurface: true, captureBeyondViewport: false });
await writeFile(output, Buffer.from(capture.data, "base64"));
socket.close();
