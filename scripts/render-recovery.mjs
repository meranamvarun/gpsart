import { readFile, writeFile } from "node:fs/promises";
import process from "node:process";

const imagePattern = /^[a-z0-9][a-z0-9./_-]*@sha256:[a-f0-9]{64}$/;

export function renderRecovery(template, config, artifacts, recovery) {
  const errors = [];
  if (!imagePattern.test(artifacts.images?.recovery ?? "")) errors.push("recovery image must be pinned by sha256 digest");
  if (recovery.backup?.retentionDays < 1 || recovery.backup?.retentionDays > 7) errors.push("backup retention must be between one and seven days");
  if (recovery.backup?.accessAuditRequired !== true) errors.push("backup storage access auditing is required");
  if (config.storageEncrypted !== true) errors.push("database storage must be encrypted");
  if (errors.length) throw new Error(errors.join("\n"));
  const values = {
    NAMESPACE: config.namespace,
    ENVIRONMENT: config.environment,
    SECRET_STORE: config.secretStoreName,
    BACKUP_DATABASE_SECRET_REF: recovery.backup.databaseSecretRef,
    BACKUP_REPOSITORY_SECRET_REF: recovery.backup.repositorySecretRef,
    BACKUP_SCHEDULE: recovery.backup.schedule,
    BACKUP_RETENTION_DAYS: String(recovery.backup.retentionDays),
    RECOVERY_IMAGE: artifacts.images.recovery,
  };
  return template.replace(/__([A-Z_]+)__/g, (token, key) => {
    if (!(key in values)) throw new Error(`unresolved recovery token ${token}`);
    return values[key];
  });
}

async function main() {
  const [configPath, artifactPath, recoveryPath, outputPath] = process.argv.slice(2);
  if (!configPath || !artifactPath || !recoveryPath || !outputPath) throw new Error("usage: render-recovery <environment> <artifacts> <recovery-config> <output>");
  const [template, config, artifacts, recovery] = await Promise.all([
    readFile(new URL("../deploy/kubernetes/recovery.yaml.tmpl", import.meta.url), "utf8"),
    readFile(configPath, "utf8").then(JSON.parse),
    readFile(artifactPath, "utf8").then(JSON.parse),
    readFile(recoveryPath, "utf8").then(JSON.parse),
  ]);
  await writeFile(outputPath, renderRecovery(template, config, artifacts, recovery));
}

if (process.argv[1]?.endsWith("render-recovery.mjs")) main().catch((error) => { console.error(error.message); process.exitCode = 1; });
