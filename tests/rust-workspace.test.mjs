import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("the Rust workspace contains the required independently-owned boundaries", async () => {
  const workspace = await read("Cargo.toml");
  for (const crate of ["gpsart-domain", "gpsart-api", "gpsart-persistence", "gpsart-routing", "gpsart-worker"]) {
    assert.match(workspace, new RegExp(`crates/${crate}`));
  }
  const domain = await read("crates/gpsart-domain/Cargo.toml");
  assert.doesNotMatch(domain, /axum|sqlx|gpsart-routing|tokio-postgres/);
});

test("CI enforces Rust, frontend, and secret-scanning gates", async () => {
  const workflow = await read(".github/workflows/ci.yml");
  for (const command of ["cargo fmt", "cargo clippy", "cargo test", "npm run factory:verify", "gitleaks"]) {
    assert.match(workflow, new RegExp(command));
  }
});

test("both binaries register shutdown handling", async () => {
  assert.match(await read("crates/gpsart-api/src/lib.rs"), /with_graceful_shutdown/);
  assert.match(await read("crates/gpsart-api/src/main.rs"), /ctrl_c/);
  assert.match(await read("crates/gpsart-worker/src/main.rs"), /ctrl_c/);
});
