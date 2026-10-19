import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import {
  assessRollback,
  validateRecoveryConfig,
  validateRecoveryEvidence,
} from "../scripts/verify-recovery.mjs";
import { renderRecovery } from "../scripts/render-recovery.mjs";
import { renderRecoveryRehearsal } from "../scripts/render-recovery-rehearsal.mjs";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");
const json = async (path) => JSON.parse(await read(path));

test("automated backups are encrypted, audited, retained at most seven days, and monitored safely", async () => {
  const [config, template, image, backup, alerts] = await Promise.all([
    json("ops/recovery/production.json"),
    read("deploy/kubernetes/recovery.yaml.tmpl"),
    read("deploy/images/recovery.Containerfile"),
    read("deploy/recovery/backup.sh"),
    json("ops/observability/alerts.json"),
  ]);

  assert.deepEqual(validateRecoveryConfig(config), []);
  assert.equal(config.backup.encryption, "restic-aes256-poly1305");
  assert.equal(config.backup.accessAuditRequired, true);
  assert.ok(config.backup.retentionDays > 0 && config.backup.retentionDays <= 7);
  assert.match(template, /kind: CronJob/);
  assert.match(template, /concurrencyPolicy: Forbid/);
  assert.match(template, /automountServiceAccountToken: false/);
  assert.match(image, /restic\/restic@sha256:[a-f0-9]{64}/);
  assert.match(image, /postgis\/postgis:16-3\.4@sha256:[a-f0-9]{64}/);
  assert.match(backup, /pg_dump/);
  assert.doesNotMatch(backup, /pg_dump[^\n]*\|[^\n]*restic backup/);
  assert.match(backup, /pg_dump[^\n]*--file/);
  assert.match(backup, /restic backup[^\n]*gpsart\.dump/);
  assert.match(backup, /forget --keep-within "\$\{GPSART_BACKUP_RETENTION_DAYS\}d" --prune/);
  assert.doesNotMatch(backup, /set -x|echo .*DATABASE_URL|echo .*RESTIC_PASSWORD/);

  const recoveryRules = alerts.rules.filter((rule) => rule.id.startsWith("backup_"));
  assert.deepEqual(recoveryRules.map(({ id }) => id).sort(), ["backup_failed", "backup_stale"]);
  for (const rule of recoveryRules) {
    assert.doesNotMatch(rule.query, /coordinate|geometry|trace|route|session|job_id|cookie|token/i);
  }
});

test("restore rehearsal remains isolated and verifies checksums, readable snapshots, ownership, and expiry cleanup", async () => {
  const [workflow, recovery, audit, template, backupTemplate, environment, recoveryConfig] = await Promise.all([
    read(".github/workflows/recovery-rehearsal.yml"),
    read("deploy/recovery/rehearse.sh"),
    read("crates/gpsart-persistence/src/bin/gpsart-recovery-audit.rs"),
    read("deploy/kubernetes/recovery-rehearsal.yaml.tmpl"),
    read("deploy/kubernetes/recovery.yaml.tmpl"),
    json("deploy/environments/production.json"),
    json("ops/recovery/production.json"),
  ]);

  assert.match(workflow, /environment: disaster-recovery/);
  assert.match(workflow, /gpsart-recovery-/);
  assert.match(workflow, /NetworkPolicy/);
  assert.doesNotMatch(workflow, /Ingress|LoadBalancer/);
  assert.match(recovery, /restic restore/);
  assert.match(recovery, /pg_restore/);
  assert.match(recovery, /gpsart-migrate/);
  assert.match(recovery, /gpsart-recovery-audit/);
  assert.match(recovery, /backup_age_seconds/);
  assert.match(audit, /migration_status/);
  assert.match(audit, /PostgresGuestSessionStore/);
  assert.match(audit, /route_candidate_snapshots/);
  assert.match(audit, /ownership_mismatches/);
  assert.match(audit, /LEFT JOIN generation_job_outcomes/);
  assert.match(audit, /--repository-age/);
  assert.doesNotMatch(audit, /ST_AsText|canonical_route|requested_pin|token_hash/);

  const digest = `sha256:${"a".repeat(64)}`;
  const artifacts = { images: { recovery: `registry.test/gpsart/recovery@${digest}` } };
  const rendered = renderRecoveryRehearsal(template, environment, artifacts, recoveryConfig, "gpsart-recovery-123");
  assert.match(rendered, /kind: NetworkPolicy/);
  assert.match(rendered, /fsGroup: 999/);
  assert.doesNotMatch(rendered, /kind: (?:Ingress|LoadBalancer)/);
  assert.doesNotMatch(rendered, /__[A-Z_]+__/);
  const backupRendered = renderRecovery(backupTemplate, environment, artifacts, recoveryConfig);
  assert.match(backupRendered, /kind: CronJob/);
  assert.doesNotMatch(backupRendered, /__[A-Z_]+__/);
  assert.throws(
    () => renderRecovery(backupTemplate, environment, artifacts, { ...recoveryConfig, backup: { ...recoveryConfig.backup, retentionDays: 8 } }),
    /retention/,
  );
});

test("failure injection and publication invariants are mandatory recovery evidence", async () => {
  const matrix = await json("ops/recovery/failure-injection.json");
  const required = new Set([
    "database_interruption", "api_restart", "worker_termination", "stale_lease",
    "partial_candidate_persistence", "network_partition",
  ]);
  assert.deepEqual(new Set(matrix.scenarios.map(({ id }) => id)), required);
  for (const scenario of matrix.scenarios) {
    assert.ok(scenario.command.length > 0);
    assert.ok(scenario.assertions.includes("no_duplicate_publication"));
    assert.ok(scenario.assertions.includes("no_corrupted_snapshot"));
    assert.ok(scenario.assertions.includes("no_false_success"));
  }
});

test("rollback is schema-aware, bounded, and fails closed outside its compatibility window", async () => {
  const policy = await json("ops/recovery/rollback-compatibility.json");
  const reviewedRevision = "9e302359abf25b615215e9c21fb6362260d02223";
  assert.deepEqual(assessRollback(policy, { currentSchema: 9, targetRevision: reviewedRevision }), {
    allowed: true,
    reason: "compatible",
  });
  assert.deepEqual(assessRollback(policy, { currentSchema: 10, targetRevision: reviewedRevision }), {
    allowed: false,
    reason: "schema_outside_compatibility_window",
  });
  assert.throws(() => assessRollback(policy, { currentSchema: 9, targetRevision: "unknown" }), /unknown rollback target/);

  const workflow = await read(".github/workflows/rollback.yml");
  assert.match(workflow, /verify-recovery\.mjs rollback/);
  assert.match(workflow, /current-schema/);
  assert.match(workflow, /target-revision="\$\{revision\}"/);
  assert.match(workflow, /gh run view/);
  assert.match(workflow, /workflowName/);
  assert.match(workflow, /stop.*forward repair/i);
});

test("recovery rehearsal binds a successful delivery run to its exact source revision", async () => {
  const workflow = await read(".github/workflows/recovery-rehearsal.yml");
  assert.match(workflow, /gh run view/);
  assert.match(workflow, /workflowName/);
  assert.match(workflow, /git checkout --detach "\$\{revision\}"/);
});

test("local recovery evidence records measurements but cannot impersonate accountable approvals", async () => {
  const evidence = await json("ops/recovery/local-rehearsal-evidence.json");
  assert.deepEqual(validateRecoveryEvidence(evidence), [
    "database_owner approval is required",
    "operations_owner approval is required",
    "privacy_owner approval is required",
  ]);
  assert.ok(evidence.measurements.rpoSeconds >= 0);
  assert.ok(evidence.measurements.rtoSeconds > 0);
  assert.equal(evidence.restore.trafficAdmittedBeforeVerification, false);
  assert.equal(evidence.restore.expiredDataRemaining, 0);
  assert.deepEqual(evidence.approvals, []);
  assert.doesNotMatch(JSON.stringify(evidence), /latitude|longitude|geometry|trace|cookie|token|credential|databaseUrl|objectName/i);
});
