import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";

const root = resolve(new URL("../", import.meta.url).pathname.replace(/^\/(\w:)/, "$1"));
const temporaryDirectory = await mkdtemp(join(tmpdir(), "gpsart-connected-"));
const transcript = join(temporaryDirectory, "live-route.json");

function run(command, args, env = process.env) {
  return new Promise((resolveRun, reject) => {
    const child = spawn(command, args, { cwd: root, env, stdio: "inherit", shell: false });
    child.on("error", reject);
    child.on("exit", (code) => code === 0
      ? resolveRun()
      : reject(new Error(`${command} exited with ${code ?? "no status"}`)));
  });
}

try {
  await run(process.execPath, [
    "scripts/cargo.mjs",
    "test",
    "-p",
    "gpsart-worker",
    "--test",
    "connected_vertical_slice",
    "--",
    "--test-threads=1",
  ], { ...process.env, GPSART_CONNECTED_TRANSCRIPT_PATH: transcript });
  await run(process.execPath, [
    "node_modules/tsx/dist/cli.mjs",
    "scripts/verify-connected-browser.mts",
    transcript,
  ]);
} finally {
  await rm(temporaryDirectory, { recursive: true, force: true });
}
