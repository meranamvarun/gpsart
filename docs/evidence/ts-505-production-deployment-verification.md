# TS-505 production deployment verification

## Acceptance evidence

1. Four OCI definitions target `linux/amd64` in delivery, use digest-pinned build/runtime bases, consume `Cargo.lock` or `package-lock.json`, and run the API, worker, and migration binary as UID 65532. The API and worker explicitly handle Unix SIGTERM through their graceful shutdown paths. API HTTP probes and the worker database/graph/current-migration `--check` probe are wired into Kubernetes.
2. `deploy/kubernetes/stack.yaml.tmpl` provisions client edge, API, functional worker, PostgreSQL 16/PostGIS 3.4, retained persistent storage, and separate staging/production namespaces. The renderer requires `storageEncrypted: true`; the platform owner must supply the reviewed encrypted StorageClass.
3. TLS ingress exposes only the client-edge service. Its nginx configuration serves the immutable client and proxies `/api/*` and health checks to the API at the same host, caps bodies/timeouts, applies security headers, and uses a log format without query strings, bodies, cookies, authorization/CSRF headers, coordinates, or traces. The connected smoke verifies HTTPS, readiness, catalogue, security headers, hostile-Origin rejection, same-origin acceptance, and Secure-cookie behavior.
4. External Secrets supplies distinct runtime, migration, and Postgres bootstrap records. Only the separate migration Job receives the forward-only migration role. Contract tests reject credential-like material in image definitions and mutable/non-digest application artifacts.
5. The delivery workflow records the full source revision, four image digests, four SPDX SBOM digests, and maximum provenance; deploys staging first; applies foundation, migration, and runtime in order; waits for readiness, rollouts, and connected smoke; then gates production through the protected environment. The worker image entrypoint is the production `gpsart-worker`, not a no-op. Rollback consumes only the runtime phase of a prior approved artifact manifest without executing its historical migrator or reversing migrations.
6. `docs/runbooks/production-deployment.md` names accountable roles, clean-host prerequisites, staging and production steps, health checks, rollback conditions, evidence retention, and teardown protections.

## Test-first record

`tests/deployment-contract.test.mjs` was executed before implementation and failed six of six checks because the image definitions, template, renderer, workflows, runbook, handoff, and evidence did not exist. The production files were then added to satisfy those executable checks.

## Staging deployment record

No external Kubernetes cluster or canonical DNS zone is available in this local worktree, so this source verification does not claim a live staging deployment. The protected staging workflow creates `staging-deployment-record.json` only after the migration, all rollouts, and connected HTTPS smoke pass. That generated record, together with the workflow URL and artifact manifest, is the authoritative staging deployment record and must exist before production approval.

## Rollback rehearsal

The source-level rehearsal renders only full Git revisions and digest-addressed `linux/amd64` images, and the rollback workflow retrieves the prior approved manifest rather than a tag. It selects only resources labelled as the runtime phase, so it cannot create the historical manifest's migration Job. A live cluster rollback has not been fabricated in this local evidence; the release owner must attach the successful protected-environment rollback run before treating operational rollback as rehearsed.

## Verification commands

```text
node --test tests/deployment-contract.test.mjs
npm run test:factory
npm run factory:verify
```

Developer verification on 2026-10-18 passed the focused deployment contract 8/8 and the complete database-backed factory gate in 62.4 seconds. Independent QA (`codex/qa_ts505`) inspected the acceptance criteria, fixed and recorded INC-060 through INC-064, then passed the focused contract 8/8 and the complete database-backed factory gate in 70.5 seconds. QA also parsed all 18 rendered Kubernetes documents with exactly one release phase each, parsed both workflow files, resolved every pinned base image and confirmed its `linux/amd64` availability, and found `git diff --check` clean.

Independent QA approved the source implementation but rejected story closure until the workflow actually builds the Linux images and SBOMs, deploys protected staging, passes the connected canonical-HTTPS smoke, and completes a live rollback rehearsal. Docker/Buildx, kubectl, a staging cluster, and canonical DNS/TLS were unavailable in this local environment. `factory/stories.json` therefore records TS-505 as blocked rather than done.

The story remains bounded by the repository-wide `NO-GO` release decision and later performance, monitoring, recovery, provider, benchmark, and browser gates. Packaging does not imply public-release approval.
