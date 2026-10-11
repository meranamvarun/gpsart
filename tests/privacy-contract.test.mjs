import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const sources = [
  "../crates/gpsart-api/src/main.rs",
  "../crates/gpsart-api/src/lib.rs",
  "../crates/gpsart-worker/src/main.rs",
  "../crates/gpsart-worker/src/lib.rs",
  "../crates/gpsart-observability/src/lib.rs",
  "../crates/gpsart-persistence/src/lib.rs",
  "../crates/gpsart-persistence/src/jobs.rs",
  "../crates/gpsart-routing/src/heart.rs",
];

test("structured application logs cannot emit protected route or credential fields", async () => {
  const protectedNames = /requested_(?:point|pin)|confirmed_start|canonical_route|geometry|full_trace|cookie|csrf|token|credential|custom_text/i;
  for (const relative of sources) {
    const text = await readFile(new URL(relative, import.meta.url), "utf8");
    const events = text.match(/tracing::(?:trace|debug|info|warn|error)!\([\s\S]*?\);/g) ?? [];
    for (const event of events) {
      assert.doesNotMatch(event, protectedNames, `${relative} has a protected structured-log field`);
    }
  }
});

test("secrets retain a redacted Debug representation", async () => {
  const config = await readFile(new URL("../crates/gpsart-config/src/lib.rs", import.meta.url), "utf8");
  assert.match(config, /formatter\.write_str\("\[REDACTED\]"\)/);
  assert.doesNotMatch(config, /derive\([^)]*Debug[^)]*\)[\s\S]{0,80}struct SecretString/);
});
