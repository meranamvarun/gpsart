import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

const isPositiveInteger = (value) => Number.isInteger(value) && value > 0;

export function validateRecoveryConfig(config) {
  const errors = [];
  if (config.version !== 1) errors.push("unsupported recovery config version");
  if (config.backup?.encryption !== "restic-aes256-poly1305") errors.push("backup encryption must be restic-aes256-poly1305");
  if (config.backup?.accessAuditRequired !== true) errors.push("backup storage access auditing is required");
  if (!isPositiveInteger(config.backup?.retentionDays) || config.backup.retentionDays > 7) errors.push("backup retention must be between one and seven days");
  if (!config.backup?.repositorySecretRef || !config.backup?.databaseSecretRef) errors.push("backup secret references are required");
  if (!isPositiveInteger(config.objectives?.maximumRpoSeconds)) errors.push("maximum RPO is required");
  if (!isPositiveInteger(config.objectives?.maximumRtoSeconds)) errors.push("maximum RTO is required");
  if (!Array.isArray(config.owners) || !["database_owner", "operations_owner", "privacy_owner"].every((role) => config.owners.includes(role))) {
    errors.push("database, operations, and privacy owners are required");
  }
  return errors;
}

export function validateRecoveryEvidence(evidence) {
  const errors = [];
  if (evidence.scope !== "isolated_recovery_rehearsal") errors.push("evidence must come from an isolated recovery rehearsal");
  if (!Number.isFinite(evidence.measurements?.rpoSeconds) || evidence.measurements.rpoSeconds < 0) errors.push("measured RPO is required");
  if (!Number.isFinite(evidence.measurements?.rtoSeconds) || evidence.measurements.rtoSeconds <= 0) errors.push("measured RTO is required");
  if (evidence.restore?.trafficAdmittedBeforeVerification !== false) errors.push("restore must remain isolated until verification finishes");
  for (const check of ["migrationChecksums", "snapshotReadability", "authorizationIsolation", "expiryCleanup"]) {
    if (evidence.restore?.checks?.[check] !== "passed") errors.push(`${check} must pass`);
  }
  if (evidence.restore?.expiredDataRemaining !== 0) errors.push("expired restored data must be deleted");
  const approvals = new Map((evidence.approvals ?? []).map((approval) => [approval.role, approval]));
  for (const role of ["database_owner", "operations_owner", "privacy_owner"]) {
    const approval = approvals.get(role);
    if (!approval?.approvedBy || approval.decision !== "approved" || !/^\d{4}-\d{2}-\d{2}T/.test(approval.approvedAt ?? "")) errors.push(`${role} approval is required`);
  }
  return errors;
}

export function assessRollback(policy, { currentSchema, targetRevision }) {
  const target = policy.targets?.[targetRevision];
  if (!target) throw new Error(`unknown rollback target ${targetRevision}`);
  if (!Number.isInteger(currentSchema)) throw new Error("current schema must be an integer");
  if (currentSchema < target.minimumSchema || currentSchema > target.maximumSchema) {
    return { allowed: false, reason: "schema_outside_compatibility_window" };
  }
  return { allowed: true, reason: "compatible" };
}

async function main() {
  const [mode, ...args] = process.argv.slice(2);
  const root = new URL("../", import.meta.url);
  if (mode === "rollback") {
    const values = Object.fromEntries(args.map((arg) => arg.split(/=(.*)/s, 2)));
    const policy = JSON.parse(await readFile(new URL("ops/recovery/rollback-compatibility.json", root), "utf8"));
    const result = assessRollback(policy, {
      currentSchema: Number(values["--current-schema"]),
      targetRevision: values["--target-revision"],
    });
    if (!result.allowed) throw new Error(`${result.reason}: stop rollback and execute reviewed forward repair`);
    console.log("rollback compatibility passed");
    return;
  }
  const config = JSON.parse(await readFile(new URL("ops/recovery/production.json", root), "utf8"));
  const errors = validateRecoveryConfig(config);
  if (errors.length) throw new Error(errors.join("\n"));
  console.log("recovery configuration passed");
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
