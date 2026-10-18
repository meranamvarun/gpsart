import { readFile, writeFile } from "node:fs/promises";
import process from "node:process";

const imagePattern = /^[a-z0-9][a-z0-9./_-]*@sha256:[a-f0-9]{64}$/;
const revisionPattern = /^[a-f0-9]{40}$/;
const dnsPattern = /^(?=.{1,253}$)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63}$/;

export function renderDeployment(template, config, artifacts, environment = process.env) {
  if (config.platform !== "linux/amd64" || artifacts.platform !== "linux/amd64") {
    throw new Error("deployment artifacts and environment must target linux/amd64");
  }
  if (config.storageEncrypted !== true) throw new Error("storageEncrypted must be true");
  if (!revisionPattern.test(artifacts.sourceRevision ?? "")) throw new Error("sourceRevision must be a full Git SHA");
  const hostname = environment[config.hostnameVariable];
  if (!dnsPattern.test(hostname ?? "")) throw new Error(`${config.hostnameVariable} must contain a canonical DNS hostname`);
  for (const name of ["api", "worker", "migrate", "clientEdge"]) {
    if (!imagePattern.test(artifacts.images?.[name] ?? "")) throw new Error(`${name} image must be pinned by sha256 digest`);
  }
  for (const name of ["api", "worker", "migrate", "clientEdge"]) {
    if (!/^sha256:[a-f0-9]{64}$/.test(artifacts.sboms?.[name] ?? "")) throw new Error(`${name} SBOM digest is missing`);
  }

  const values = {
    ENVIRONMENT: config.environment,
    NAMESPACE: config.namespace,
    HOSTNAME: hostname,
    PUBLIC_ORIGIN: `https://${hostname}`,
    SECRET_STORE: config.secretStoreName,
    RUNTIME_DATABASE_SECRET_REF: config.runtimeDatabaseSecretRef,
    MIGRATION_DATABASE_SECRET_REF: config.migrationDatabaseSecretRef,
    POSTGRES_BOOTSTRAP_SECRET_REF: config.postgresBootstrapSecretRef,
    STORAGE_CLASS: config.storageClassName,
    CERTIFICATE_ISSUER: config.certificateIssuer,
    EDGE_REPLICAS: String(config.replicas.edge),
    API_REPLICAS: String(config.replicas.api),
    WORKER_REPLICAS: String(config.replicas.worker),
    SOURCE_REVISION: artifacts.sourceRevision,
    REVISION_SHORT: artifacts.sourceRevision.slice(0, 12),
    API_IMAGE: artifacts.images.api,
    WORKER_IMAGE: artifacts.images.worker,
    MIGRATE_IMAGE: artifacts.images.migrate,
    CLIENT_EDGE_IMAGE: artifacts.images.clientEdge,
  };
  const rendered = template.replace(/__([A-Z_]+)__/g, (token, key) => {
    if (!(key in values)) throw new Error(`unresolved deployment token ${token}`);
    return values[key];
  });
  const unresolved = rendered.match(/__[A-Z_]+__/);
  if (unresolved) throw new Error(`unresolved deployment token ${unresolved[0]}`);
  return rendered;
}

async function main() {
  const [configPath, artifactPath, outputPath] = process.argv.slice(2);
  if (!configPath || !artifactPath || !outputPath) {
    throw new Error("usage: node scripts/render-deployment.mjs <environment.json> <artifact-manifest.json> <output.yaml>");
  }
  const [template, configText, artifactText] = await Promise.all([
    readFile(new URL("../deploy/kubernetes/stack.yaml.tmpl", import.meta.url), "utf8"),
    readFile(configPath, "utf8"),
    readFile(artifactPath, "utf8"),
  ]);
  await writeFile(outputPath, renderDeployment(template, JSON.parse(configText), JSON.parse(artifactText)));
}

if (process.argv[1]?.endsWith("render-deployment.mjs")) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
