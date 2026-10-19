import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

function compare(value, operator, threshold) {
  if (!Number.isFinite(value)) return false;
  if (operator === "gt") return value > threshold;
  if (operator === "gte") return value >= threshold;
  if (operator === "lt") return value < threshold;
  if (operator === "lte") return value <= threshold;
  throw new Error(`unsupported alert operator ${operator}`);
}

function fires(rule, values) {
  const condition = rule.condition;
  if (!(condition.metric in values)) return false;
  const denominator = condition.denominator ? Math.max(Number(values[condition.denominator] ?? 0), 1) : 1;
  const value = Number(values[condition.metric]) / denominator;
  if (!compare(value, condition.operator, condition.threshold)) return false;
  if (!condition.when_metric) return true;
  return compare(Number(values[condition.when_metric]), condition.when_operator, condition.when_threshold);
}

export function evaluateControlledSignals(alerts, signals) {
  const failures = [];
  const exercised = new Set();
  for (const scenario of signals.cases ?? []) {
    const actual = alerts.rules.filter((rule) => fires(rule, scenario.values ?? {})).map((rule) => rule.id).sort();
    const expected = [...(scenario.expected_alerts ?? [])].sort();
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      failures.push(`${scenario.id}: expected ${expected.join(",") || "none"}; got ${actual.join(",") || "none"}`);
    }
    for (const id of expected) exercised.add(id);
  }
  for (const rule of alerts.rules) if (!exercised.has(rule.id)) failures.push(`${rule.id}: no controlled firing signal`);
  return { failures, scenarios: signals.cases?.length ?? 0, rules: alerts.rules?.length ?? 0 };
}

export function validateMonitoringBundle({ alerts, signals, ownership, runbook }) {
  const errors = [];
  const ids = new Set();
  for (const rule of alerts.rules ?? []) {
    if (ids.has(rule.id)) errors.push(`${rule.id}: duplicate rule`);
    ids.add(rule.id);
    if (!ownership.owners?.[rule.owner]) errors.push(`${rule.id}: unknown owner ${rule.owner}`);
    if (!ownership.routes?.[rule.route]) errors.push(`${rule.id}: unknown route ${rule.route}`);
    if (ownership.routes?.[rule.route]?.owner !== rule.owner) errors.push(`${rule.id}: route owner mismatch`);
    if (!rule.query || !rule.condition || rule.for_seconds <= 0) errors.push(`${rule.id}: incomplete executable rule`);
    const anchor = rule.runbook?.split("#")[1]?.replaceAll("-", " ");
    if (!anchor || !runbook.toLowerCase().includes(anchor)) errors.push(`${rule.id}: missing runbook section`);
  }
  errors.push(...evaluateControlledSignals(alerts, signals).failures);
  return { errors };
}

async function main() {
  const root = new URL("../", import.meta.url);
  const [alerts, signals, ownership, runbook] = await Promise.all([
    readFile(new URL("ops/observability/alerts.json", root), "utf8").then(JSON.parse),
    readFile(new URL("ops/observability/controlled-signals.json", root), "utf8").then(JSON.parse),
    readFile(new URL("ops/observability/ownership.json", root), "utf8").then(JSON.parse),
    readFile(new URL("docs/runbooks/monitoring-incident-response.md", root), "utf8"),
  ]);
  const report = validateMonitoringBundle({ alerts, signals, ownership, runbook });
  if (report.errors.length) throw new Error(report.errors.join("\n"));
  const signalsReport = evaluateControlledSignals(alerts, signals);
  console.log(`monitoring contract passed: ${signalsReport.rules} rules, ${signalsReport.scenarios} controlled signals`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
