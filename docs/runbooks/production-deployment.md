# Production deployment runbook

This runbook deploys the complete GPSART stack to a Kubernetes cluster on `linux/amd64`. Staging and production use the same immutable images and template but separate namespaces, canonical HTTPS hosts, managed-secret paths, encrypted volumes, certificates, approvals, and deployment records.

## Owners

- Release owner: approves the full source revision and starts `Build and deliver production stack`.
- Platform owner: owns the Linux build/deploy runners, Kubernetes clusters, ingress, DNS, certificate issuers, External Secrets stores, and the `encrypted-rwo` storage classes.
- Database owner: controls the bootstrap and separately authorized migration database roles and verifies backup readiness before promotion.
- Privacy/security owner: reviews edge behavior, registry/SBOM attestations, secret access, and privacy-safe logs.
- Incident commander: decides rollback when a rollback condition is met.

Every production GitHub environment must require approval from the release and platform owners. Workload identity, not a long-lived kubeconfig committed to this repository, authorizes the environment-scoped deploy runner.

## Prerequisites

1. Provide clean `linux/amd64` build and deploy runners with Git, Node 22.15, Docker Buildx, Syft, jq, kubectl, and GitHub CLI. The build itself uses pinned BuildKit, Node, Rust, nginx, distroless, and PostGIS image digests plus `package-lock.json` and `Cargo.lock`.
2. Install Kubernetes, nginx Ingress, cert-manager, and External Secrets. Provision separate staging and production `ClusterSecretStore` and certificate issuers named by `deploy/environments/*.json`.
3. Provision an `encrypted-rwo` StorageClass in each cluster and independently verify provider-side encryption at rest. The renderer refuses configurations that do not assert encrypted storage.
4. Put three records in each managed secret store: the runtime database URL, the more privileged forward-only migration URL, and Postgres bootstrap values. Required bootstrap keys are `POSTGRES_DB=gpsart`, `POSTGRES_USER=gpsart`, `POSTGRES_PASSWORD`, and `PGDATA=/var/lib/postgresql/data/pgdata`. Never put their values in GitHub variables, image layers, manifests, tickets, or logs.
5. Set the protected environment variable `GPSART_HOSTNAME` to the canonical DNS hostname. Point DNS only after the ingress has a valid certificate. Staging and production hostnames must differ.
6. Configure registry retention so every promoted digest and attached SBOM/provenance attestation remains available for rollback. Confirm the selected source revision has passed `npm run factory:verify` with PostgreSQL-backed tests.
7. Before production promotion, confirm an encrypted, access-audited backup exists and the current release decision permits deployment. This runbook does not override a `NO-GO` release decision.

## Deploy to staging

1. Dispatch `.github/workflows/deliver.yml` with a full 40-character source revision. Do not use a branch or mutable tag.
2. The build job verifies that exact checkout, runs the complete factory gate, builds only `linux/amd64`, pushes four digest-addressed images, attaches provenance/SBOM attestations, and uploads `artifact-manifest.json` plus SPDX SBOMs.
3. The staging environment renders `deploy/kubernetes/stack.yaml.tmpl`. The renderer rejects mutable images, missing SBOM digests, wrong platforms, unencrypted storage configuration, malformed source revisions, and missing canonical hostname values.
4. The deploy job applies manifest resources labelled `foundation`, waits for PostgreSQL, then applies and waits for the separately authorized `migration` phase. Only after that succeeds does it apply the `runtime` phase. The migration pod alone receives `DATABASE_URL`; migrations are forward-only. API and worker receive only the runtime database role.
5. Wait for the migration job, API, functional worker, and client-edge rollouts. The connected smoke checks the canonical HTTPS edge, security headers, API/database readiness, catalogue decoding, Origin enforcement, and Secure guest cookie.
6. Retain `artifact-manifest.json`, all SBOMs, and `staging-deployment-record.json`. Review pod restarts, failed External Secrets, Postgres volume binding, certificate state, and privacy-safe logs before approving production.

## Promote to production

1. Require a successful staging job from the same workflow run. Production downloads the exact same artifact manifest; it does not rebuild or retag images.
2. Confirm the source revision and every artifact digest against the staging record. Confirm there is no open migration failure, readiness failure, provider review block, or release `NO-GO` that prohibits promotion.
3. Obtain protected-environment approval. The production job renders the separate production configuration, applies the forward-only migration job, waits for rollouts, and repeats the connected smoke.
4. Record the workflow URL, approvers, source revision, artifact/SBOM digests, timestamps, migration job result, rollout revisions, smoke result, and any deviations. Do not copy response bodies, cookies, database URLs, coordinates, or route traces into the record.

## Health checks

- Edge: `GET https://<canonical-host>/edge-health` returns only the static edge status.
- API liveness: `GET https://<canonical-host>/health/live` returns `200` while the process is serving.
- API readiness: `GET https://<canonical-host>/health/ready` returns `200` only when PostgreSQL and migrations are current.
- Connected API: `GET https://<canonical-host>/api/v1/designs` returns the decoded catalogue without credentials.
- Worker readiness: Kubernetes executes `/app/gpsart-worker --check`, which loads the checksum-pinned graph and connects with the runtime database configuration. The worker has no public port; process exit is its liveness failure and Kubernetes restarts it.
- Postgres: `pg_isready` must pass and the PVC must show the reviewed encrypted StorageClass. Never print the database URL.

Use `scripts/smoke-deployment.mjs` through the canonical HTTPS origin. Never bypass the edge with a public API origin because that would invalidate cookie, Origin/CSRF, security-header, and logging checks.

## Rollback

Rollback conditions are failed readiness or connected smoke, repeated crash loops, migration incompatibility with the new binary, elevated dependency failures, privacy/security-header regression, or evidence that the wrong revision/digest was deployed. Stop promotion immediately.

Dispatch `.github/workflows/rollback.yml` with the successful delivery run containing the last approved `artifact-manifest.json` and the affected environment. The workflow checks out that source, renders those exact prior digests, applies only resources labelled `runtime`, and waits for all application rollouts. It never applies the prior manifest's migration Job and does not reverse or edit database migrations: migrations are forward-only, so a binary rollback is allowed only when the previous binaries are compatible with the current schema. If they are not, keep traffic stopped and execute a reviewed forward repair.

After rollback, repeat all health and connected smoke checks and record the selected manifest, decision owner, condition, start/end times, and outcome without sensitive request or route data.

## Teardown protection

Production teardown is not a deployment step. The namespace is labelled and annotated as protected, StatefulSet PVC retention is `Retain` for deletion and scaling, and the workflow never deletes namespaces, PVCs, secrets, or migrations.

Any teardown requires a separately reviewed change, release owner and database owner approval, a verified encrypted backup, confirmation of retention/deletion obligations, and explicit resolved resource names. Remove public traffic first, then workloads; do not delete the PVC, managed secrets, registry digests, audit records, or backup until their accountable owner confirms the retention decision. Never run recursive or wildcard deletion commands.
