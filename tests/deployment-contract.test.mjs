import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { renderDeployment } from "../scripts/render-deployment.mjs";

const read = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("linux images are immutable, locked, non-root, and never package secrets", async () => {
  const [api, worker, migrate, client, toolchain] = await Promise.all([
    read("deploy/images/api.Containerfile"),
    read("deploy/images/worker.Containerfile"),
    read("deploy/images/migrate.Containerfile"),
    read("deploy/images/client-edge.Containerfile"),
    read("rust-toolchain.toml"),
  ]);

  assert.match(toolchain, /channel = "1\.99\.0"/);
  for (const file of [api, worker, migrate, client]) {
    assert.match(file, /^# syntax=docker\/dockerfile:1\.18@sha256:[a-f0-9]{64}/m);
    assert.doesNotMatch(file, /COPY\s+.*(?:\.env|secret|credential)/i);
  }
  for (const file of [api, worker, migrate]) {
    assert.match(file, /--locked/);
    assert.match(file, /FROM .*@sha256:[a-f0-9]{64}/);
    assert.match(file, /USER 65532:65532/);
  }
  assert.match(client, /npm ci/);
  assert.match(client, /npm run build/);
  assert.match(client, /USER 101/);
  assert.match(worker, /gpsart-worker/);
  assert.doesNotMatch(worker, /sleep\s+infinity|tail\s+-f|placeholder|no-op/i);
  const dockerignore = await read(".dockerignore");
  assert.match(dockerignore, /^\.git$/m);
  assert.match(dockerignore, /^\.tools$/m);
  assert.match(dockerignore, /^target$/m);
});

test("deployment template isolates environments and provisions the complete stack", async () => {
  const [template, staging, production] = await Promise.all([
    read("deploy/kubernetes/stack.yaml.tmpl"),
    read("deploy/environments/staging.json"),
    read("deploy/environments/production.json"),
  ]);

  for (const kind of ["Deployment", "StatefulSet", "Service", "Ingress", "ExternalSecret", "Job", "PodDisruptionBudget", "NetworkPolicy"]) {
    assert.match(template, new RegExp(`kind: ${kind}`));
  }
  for (const component of ["gpsart-client-edge", "gpsart-api", "gpsart-worker", "gpsart-postgres", "gpsart-migrate"]) {
    assert.match(template, new RegExp(component, "g"));
  }
  assert.match(template, /postgis\/postgis:16-3\.4@sha256:[a-f0-9]{64}/);
  assert.match(template, /storageClassName: __STORAGE_CLASS__/);
  assert.match(template, /GPSART_PUBLIC_ORIGIN/);
  assert.match(template, /GPSART_ROUTING_MANIFEST/);
  assert.match(template, /GPSART_COVERAGE_VERSION: "bengaluru-central-pilot-coverage-v1"/);
  assert.match(template, /runAsNonRoot: true/g);
  assert.match(template, /readOnlyRootFilesystem: true/g);
  assert.match(template, /livenessProbe:/g);
  assert.match(template, /readinessProbe:/g);
  assert.match(template, /terminationGracePeriodSeconds:/g);
  assert.match(template, /mountPath: \/tmp/);
  assert.match(template, /secretStoreRef:/);
  assert.match(template, /backoffLimit: 0/);
  assert.doesNotMatch(template, /(?:postgres(?:ql)?:\/\/|stringData:|value:\s+[^_\n])/i);

  const stagingConfig = JSON.parse(staging);
  const productionConfig = JSON.parse(production);
  assert.equal(stagingConfig.environment, "staging");
  assert.equal(productionConfig.environment, "production");
  assert.notEqual(stagingConfig.hostnameVariable, productionConfig.hostnameVariable);
  assert.notEqual(stagingConfig.runtimeDatabaseSecretRef, productionConfig.runtimeDatabaseSecretRef);
  assert.notEqual(stagingConfig.migrationDatabaseSecretRef, productionConfig.migrationDatabaseSecretRef);
  assert.equal(stagingConfig.platform, "linux/amd64");
  assert.equal(productionConfig.platform, "linux/amd64");
  assert.equal(stagingConfig.storageEncrypted, true);
  assert.equal(productionConfig.storageEncrypted, true);
});

test("the edge enforces same-origin HTTPS, bounded requests, security headers, and privacy-safe logs", async () => {
  const [edge, template, smoke] = await Promise.all([
    read("deploy/edge/nginx.conf"),
    read("deploy/kubernetes/stack.yaml.tmpl"),
    read("scripts/smoke-deployment.mjs"),
  ]);

  assert.match(edge, /location \/api\//);
  assert.match(edge, /proxy_pass http:\/\/gpsart-api:8080/);
  assert.match(edge, /client_max_body_size 64k/);
  assert.match(edge, /proxy_(?:connect|read|send)_timeout/);
  assert.match(edge, /add_header Strict-Transport-Security/);
  assert.match(edge, /add_header Content-Security-Policy/);
  assert.match(edge, /add_header X-Content-Type-Options/);
  assert.match(edge, /log_format privacy_safe/);
  const privacyLog = edge.match(/log_format privacy_safe ([^;]+);/)?.[1] ?? "";
  assert.doesNotMatch(privacyLog, /\$remote_addr|\$uri|\$request_uri|\$args|\$http_cookie|\$http_authorization|\$request_body/);
  assert.match(template, /tls:/);
  assert.match(template, /host: __HOSTNAME__/);
  assert.match(smoke, /https:\/\/cross-origin\.invalid/);
  assert.match(smoke, /status !== 403/);
});

test("renderer accepts only digest-addressed linux artifacts and emits no unresolved tokens", async () => {
  const renderer = await read("scripts/render-deployment.mjs");
  assert.match(renderer, /linux\/amd64/);
  assert.match(renderer, /@sha256:/);
  assert.match(renderer, /sourceRevision/);
  assert.match(renderer, /storageEncrypted/);
  assert.match(renderer, /unresolved deployment token/);
});

test("delivery promotes one manifest through staging and production with smoke and rollback gates", async () => {
  const [delivery, rollback] = await Promise.all([
    read(".github/workflows/deliver.yml"),
    read(".github/workflows/rollback.yml"),
  ]);

  assert.match(delivery, /--platform linux\/amd64/g);
  assert.match(delivery, /runs-on: ubuntu-24\.04/);
  assert.doesNotMatch(delivery, /runs-on: \[self-hosted, linux, x64, gpsart-build\]/);
  assert.match(delivery, /postgis\/postgis:16-3\.4@sha256:[a-f0-9]{64}/);
  assert.match(delivery, /DATABASE_URL: postgres:\/\/postgres:postgres@127\.0\.0\.1:5432\/postgres/);
  assert.match(delivery, /actions\/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020/);
  assert.match(delivery, /node-version: 22\.15\.0/);
  assert.match(delivery, /docker\/setup-buildx-action@8d2750c68a42422c14e847fe6c8ac0403b4cbd6f/);
  assert.match(delivery, /version: v0\.38\.0/);
  assert.match(delivery, /image=moby\/buildkit@sha256:[a-f0-9]{64}/);
  assert.match(delivery, /anchore\/sbom-action\/download-syft@e22c389904149dbc22b58101806040fa8d37a610/);
  assert.match(delivery, /syft-version: v1\.54\.1/);
  assert.match(delivery, /generate-sbom/);
  assert.match(delivery, /environment: staging/);
  assert.match(delivery, /needs: deploy-staging/);
  assert.match(delivery, /environment: production/);
  assert.match(delivery, /rollout status/);
  assert.match(delivery, /smoke-deployment\.mjs/);
  assert.match(delivery, /gpsart-migrate/);
  for (const environment of ["staging", "production"]) {
    const block = delivery.match(new RegExp(`name: (?:Render and deploy ${environment}|Promote exact staging artifacts to production)([\\s\\S]*?)(?=\\n      - uses:|$)`))?.[1] ?? "";
    const foundation = block.indexOf("gpsart.dev/release-phase=foundation");
    const migration = block.indexOf("gpsart.dev/release-phase=migration");
    const runtime = block.indexOf("gpsart.dev/release-phase=runtime");
    assert.ok(foundation >= 0 && migration > foundation && runtime > migration, `${environment} must migrate before starting the new runtime`);
  }
  assert.match(rollback, /artifact-manifest/);
  assert.match(rollback, /environment:/);
  assert.match(rollback, /rollout status/);
  assert.match(rollback, /gpsart\.dev\/release-phase=runtime/);
  assert.doesNotMatch(rollback, /gpsart\.dev\/release-phase=migration/);
});

test("linux processes handle SIGTERM and the worker readiness check verifies current migrations", async () => {
  const [apiMain, workerMain] = await Promise.all([
    read("crates/gpsart-api/src/main.rs"),
    read("crates/gpsart-worker/src/main.rs"),
  ]);
  for (const source of [apiMain, workerMain]) {
    assert.match(source, /SignalKind::terminate/);
    assert.match(source, /shutdown_signal\(\)\.await/);
  }
  assert.match(workerMain, /DependencyProbe/);
  assert.match(workerMain, /probe\.check\(\)\.await/);
});

test("renderer binds exact environment and artifact values and rejects unsafe inputs", async () => {
  const [template, configText] = await Promise.all([
    read("deploy/kubernetes/stack.yaml.tmpl"),
    read("deploy/environments/staging.json"),
  ]);
  const config = JSON.parse(configText);
  const digest = `sha256:${"a".repeat(64)}`;
  const artifacts = {
    sourceRevision: "b".repeat(40),
    platform: "linux/amd64",
    images: {
      api: `registry.test/gpsart/api@${digest}`,
      worker: `registry.test/gpsart/worker@${digest}`,
      migrate: `registry.test/gpsart/migrate@${digest}`,
      clientEdge: `registry.test/gpsart/client-edge@${digest}`,
    },
    sboms: { api: digest, worker: digest, migrate: digest, clientEdge: digest },
  };
  const rendered = renderDeployment(template, config, artifacts, { GPSART_STAGING_HOSTNAME: "staging.gpsart.test" });
  assert.doesNotMatch(rendered, /__[A-Z_]+__/);
  assert.match(rendered, /https:\/\/staging\.gpsart\.test/);
  assert.match(rendered, new RegExp(artifacts.images.worker));

  assert.throws(
    () => renderDeployment(template, config, { ...artifacts, images: { ...artifacts.images, worker: "registry.test/gpsart/worker:latest" } }, { GPSART_STAGING_HOSTNAME: "staging.gpsart.test" }),
    /pinned by sha256 digest/,
  );
  assert.throws(
    () => renderDeployment(template, { ...config, storageEncrypted: false }, artifacts, { GPSART_STAGING_HOSTNAME: "staging.gpsart.test" }),
    /storageEncrypted/,
  );
});

test("runbook and evidence name operational ownership, checks, rollback, and teardown protection", async () => {
  const [runbook, evidence, handoff] = await Promise.all([
    read("docs/runbooks/production-deployment.md"),
    read("docs/evidence/ts-505-production-deployment-verification.md"),
    read("factory/handoffs/TS-505-production-deployment.md"),
  ]);

  for (const heading of ["Owners", "Prerequisites", "Deploy to staging", "Promote to production", "Health checks", "Rollback", "Teardown protection"]) {
    assert.match(runbook, new RegExp(`## ${heading}`));
  }
  assert.match(runbook, /forward-only/i);
  assert.match(runbook, /source revision/i);
  assert.match(runbook, /artifact digest/i);
  assert.match(evidence, /staging deployment record/i);
  assert.match(evidence, /rollback rehearsal/i);
  assert.match(handoff, /Consumers:/);
  assert.match(handoff, /Compatibility:/);
  assert.match(handoff, /Verification/);
});
