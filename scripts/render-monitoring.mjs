import { readFile, writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

const safeName = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
const revisionPattern = /^[a-f0-9]{40}$/;
const indent = (value, spaces) => value.split("\n").map((line) => `${" ".repeat(spaces)}${line}`).join("\n");
const yamlString = (value) => JSON.stringify(String(value));

export function renderMonitoring(template, config, artifacts, dashboard, alerts, collector) {
  for (const key of ["namespace", "monitoringOwner", "alertReceiverSecretRef", "deadmanReceiverSecretRef"]) {
    if (!safeName.test(config[key] ?? "")) throw new Error(`${key} must be a safe Kubernetes name`);
  }
  if (!revisionPattern.test(artifacts.sourceRevision ?? "")) throw new Error("sourceRevision must be a full Git SHA");
  if (!/^https:\/\//.test(config.runbookBaseUrl ?? "")) throw new Error("runbookBaseUrl must use HTTPS");
  for (const rule of alerts.rules) {
    if (rule.owner === "gpsart-oncall" && rule.owner !== config.monitoringOwner) throw new Error("monitoringOwner does not own application alerts");
  }
  const runbookBase = config.runbookBaseUrl.replace("{source_revision}", artifacts.sourceRevision).replace(/\/$/, "");
  const rules = alerts.rules.flatMap((rule) => [
    `        - alert: ${rule.id}`,
    `          expr: ${yamlString(rule.query)}`,
    `          for: ${rule.for_seconds}s`,
    "          labels:",
    `            severity: ${yamlString(rule.severity)}`,
    `            owner: ${yamlString(rule.owner)}`,
    `            route: ${yamlString(rule.route)}`,
    "          annotations:",
    `            summary: ${yamlString(`GPSART ${rule.id.replaceAll("_", " ")}`)}`,
    `            runbook_url: ${yamlString(`${runbookBase}#${rule.runbook.split("#")[1]}`)}`,
  ]).join("\n");
  const values = {
    NAMESPACE: config.namespace,
    ALERT_RECEIVER_SECRET: config.alertReceiverSecretRef,
    DEADMAN_RECEIVER_SECRET: config.deadmanReceiverSecretRef,
    DASHBOARD_JSON: indent(JSON.stringify(dashboard, null, 2), 4),
    COLLECTOR_JSON: indent(JSON.stringify(collector, null, 2), 4),
    PROMETHEUS_RULES: rules,
  };
  const rendered = template.replace(/__([A-Z_]+)__/g, (token, key) => {
    if (!(key in values)) throw new Error(`unresolved monitoring token ${token}`);
    return values[key];
  });
  if (/__[A-Z_]+__/.test(rendered)) throw new Error("unresolved monitoring token");
  return rendered;
}

async function main() {
  const [configPath, artifactPath, outputPath] = process.argv.slice(2);
  if (!configPath || !artifactPath || !outputPath) {
    throw new Error("usage: node scripts/render-monitoring.mjs <environment.json> <artifact-manifest.json> <output.yaml>");
  }
  const root = new URL("../", import.meta.url);
  const [template, config, artifacts, dashboard, alerts, collector] = await Promise.all([
    readFile(new URL("deploy/kubernetes/monitoring.yaml.tmpl", root), "utf8"),
    readFile(configPath, "utf8").then(JSON.parse),
    readFile(artifactPath, "utf8").then(JSON.parse),
    readFile(new URL("ops/observability/dashboard.json", root), "utf8").then(JSON.parse),
    readFile(new URL("ops/observability/alerts.json", root), "utf8").then(JSON.parse),
    readFile(new URL("ops/observability/collector-contract.json", root), "utf8").then(JSON.parse),
  ]);
  await writeFile(outputPath, renderMonitoring(template, config, artifacts, dashboard, alerts, collector));
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
