import { readFile, writeFile } from "node:fs/promises";
import process from "node:process";

const imagePattern = /^[a-z0-9][a-z0-9./_-]*@sha256:[a-f0-9]{64}$/;
const namespacePattern = /^gpsart-recovery-[a-z0-9-]{1,40}$/;

export function renderRecoveryRehearsal(template, config, artifacts, recovery, namespace) {
  if (!namespacePattern.test(namespace)) throw new Error("isolated recovery namespace is invalid");
  if (!imagePattern.test(artifacts.images?.recovery ?? "")) throw new Error("recovery image must be digest pinned");
  const values = {
    RECOVERY_NAMESPACE: namespace,
    SECRET_STORE: config.secretStoreName,
    POSTGRES_BOOTSTRAP_SECRET_REF: config.postgresBootstrapSecretRef,
    BACKUP_REPOSITORY_SECRET_REF: recovery.backup.repositorySecretRef,
    RECOVERY_IMAGE: artifacts.images.recovery,
  };
  return template.replace(/__([A-Z_]+)__/g, (token, key) => {
    if (!(key in values)) throw new Error(`unresolved recovery rehearsal token ${token}`);
    return values[key];
  });
}

async function main() {
  const [configPath, artifactPath, recoveryPath, namespace, outputPath] = process.argv.slice(2);
  if (!outputPath) throw new Error("usage: render-recovery-rehearsal <environment> <artifacts> <recovery-config> <namespace> <output>");
  const [template, config, artifacts, recovery] = await Promise.all([
    readFile(new URL("../deploy/kubernetes/recovery-rehearsal.yaml.tmpl", import.meta.url), "utf8"),
    readFile(configPath, "utf8").then(JSON.parse),
    readFile(artifactPath, "utf8").then(JSON.parse),
    readFile(recoveryPath, "utf8").then(JSON.parse),
  ]);
  await writeFile(outputPath, renderRecoveryRehearsal(template, config, artifacts, recovery, namespace));
}

if (process.argv[1]?.endsWith("render-recovery-rehearsal.mjs")) main().catch((error) => { console.error(error.message); process.exitCode = 1; });
