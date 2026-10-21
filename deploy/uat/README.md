# Google Cloud UAT deployment

This directory documents the constrained, single-VM UAT topology. It is not the supported production target: production remains the digest-pinned Kubernetes deployment described in `docs/runbooks/production-deployment.md`.

## What is committed

- `docker-compose.example.yml` is a secret-free runtime template. Copy it to the ignored `docker-compose.yml` on the VM and replace every `GPSART_*_IMAGE` placeholder with a full Artifact Registry image digest.
- `Caddyfile.example` provides same-origin HTTPS. The API's secure guest cookie and strict Origin/CSRF validation require a canonical HTTPS hostname; do not use a bare IP address.
- The PostGIS image is the repository's checked-in digest. Application images must be rebuilt and deployed by digest for every source revision.

## What must never be committed

The local `.gitignore` prevents these names from entering Git:

- `.env`: `POSTGRES_PASSWORD`, the UAT host, and concrete image references;
- `docker-compose.yml` and `Caddyfile`: mutable host-specific runtime settings;
- `deployment-record.local.md`: cloud project, VM, public endpoint, service-account, deploy timestamp, image digests, and observed smoke results.

Do not commit database URLs, database passwords, cookies, access tokens, exact user coordinates, route traces, Caddy certificates, or Docker volumes.

## Deployment sequence

1. Build every API/worker/migration/edge image from one source revision on a Linux/amd64 builder. Run `node --test tests/deployment-contract.test.mjs` before build, and record the resulting image digests.
2. Give the VM service account only `roles/artifactregistry.reader`; configure Docker registry credentials with the VM identity. Do not copy a user credential file or service-account key to the VM.
3. Create the ignored `.env`, `docker-compose.yml`, and `Caddyfile` from the examples. Generate a hexadecimal database password locally; keep PostgreSQL off the public network.
4. Pull images, start PostgreSQL, wait for `pg_isready`, then run `docker-compose run --rm migrate`. The migration binary requires `DATABASE_URL` (not `GPSART_DATABASE_URL`).
5. Start API, edge, and Caddy. Wait for `GET /health/ready` through the public HTTPS origin before starting the worker.
6. Run edge, liveness, readiness, catalogue, origin/CSRF, cookie, and worker checks through the canonical public origin. Record only status codes and non-sensitive outcomes.

## Failure patterns found during this UAT

- A client-edge image must copy `worker/` and `.openai/` because `npm run build` prepares the optional Sites output.
- The API image must copy `scripts/extract-osm-pedestrian-pilot-snapshot.mjs`; startup validates its hash through the approved routing manifest.
- `sudo docker` uses root's Docker credentials. After adding the SSH user to the `docker` group, use `docker` without `sudo` so Artifact Registry credential-helper authentication works.
- A recurring `MigrationsNotCurrent` or `ManifestInvalid` API restart is fail-closed behavior. Do not bypass it; inspect the migration result and image contents.

## UAT limits

An Always Free `e2-micro` has approximately 1 GiB RAM. This topology is for low-volume, supervised UAT only; it is not performance-tested, highly available, backed up, or production-approved. The worker must remain stopped until API readiness is successful.
