# GPSART

GPSART is a Bengaluru-pilot GPS-art route planner. The repository contains:

- a TypeScript/Vite browser client;
- a Rust/Axum HTTP API;
- a Rust worker and routing/domain libraries;
- PostgreSQL/PostGIS persistence and forward-only migrations;
- a pinned, local OpenStreetMap-derived pilot graph; and
- privacy, observability, accessibility, and release verification checks.

## Important release status

This revision is **not approved for a public production release**. The signed release decision in
[`docs/release/first-release-checklist.json`](docs/release/first-release-checklist.json) is `no_go`.
The current blockers are summarized in [`docs/release/limitations.md`](docs/release/limitations.md).

The production worker now leases queued jobs, runs the checksum-pinned local routing graph, and
publishes lease-fenced candidate snapshots. That implementation is not by itself a production
approval: live deployment evidence, performance, recovery, provider, and
independent release gates listed in the release checklist still remain fail-closed.

Google sign-in and place search are also UI concepts only. Guest sessions are implemented; Google
OAuth and a geocoder are not connected. Device-saved routes use IndexedDB and remain local to the
browser.

## Architecture

```text
Browser (one HTTPS origin)
  |-- static files ----------> dist/client
  `-- /api/v1/* -------------> gpsart-api :8080
                                  |
                                  `-------> PostgreSQL 16 + PostGIS

gpsart-worker ----------------------------> PostgreSQL 16 + PostGIS
  (leases queued jobs and publishes only validated, lease-fenced snapshots)
```

The browser deliberately uses relative `/api/v1/...` URLs. In a connected environment, static files
and API requests must therefore be exposed through the same HTTPS origin. This is also required for
the Secure guest-session cookie and exact Origin/CSRF checks.

## Prerequisites

- Node.js 22 and npm (`npm ci` is the supported dependency install)
- the stable Rust toolchain with `rustfmt` and `clippy` (see `rust-toolchain.toml`)
- PostgreSQL 16 with PostGIS 3.4 for a connected API
- SQLx CLI 0.8.6 for applying migrations
- an HTTPS reverse proxy such as Caddy or nginx for a connected browser session
- a Chromium/Chrome executable only when running the accessibility audit

The CI reference database image is `postgis/postgis:16-3.4`. Other compatible PostgreSQL/PostGIS
installations may work, but are not the checked-in CI baseline.

Install the Rust and migration tooling if needed:

```sh
rustup toolchain install stable --profile minimal --component rustfmt --component clippy
cargo install sqlx-cli --version 0.8.6 --no-default-features --features rustls,postgres
```

## Fastest local start: frontend only

This mode is useful for UI work. API-dependent controls show their explicit unavailable states; it is
not an end-to-end application run.

```sh
npm ci
npm run dev
```

Open the URL printed by Vite (normally `http://localhost:5173`). To expose the development-only,
visibly labelled design-review screens:

```powershell
$env:VITE_GPSART_DESIGN_REVIEW = "true"
npm run dev
```

Production builds never include the development-only review route, regardless of that variable.

## Connected local stack

The following setup exercises the real browser client, guest sessions, API, and PostGIS. It still has
the worker limitation described above.

### 1. Install dependencies and build

```sh
npm ci
npm run build
cargo build --workspace
```

`npm run build` performs TypeScript checking, builds the client into `dist/client`, scans the
production bundle for forbidden prototype fixtures, and prepares the optional Sites hosting output.

### 2. Create and migrate the database

Create a PostgreSQL database whose owner can enable PostGIS. For example, with local PostgreSQL:

```sh
createdb -U postgres gpsart
```

Set the migration URL and run the repository's forward-only upgrader before starting either Rust process:

```powershell
$env:DATABASE_URL = "postgres://postgres:change-me@127.0.0.1:5432/gpsart"
cargo run -p gpsart-persistence --bin gpsart-migrate
```

The API and worker refuse to become ready if migrations are missing, pending, changed, or
incompatible. Do not apply the SQL files manually: SQLx migration metadata and checksums are part of
the readiness contract.

### 3. Configure the API

The API has no secret-bearing defaults. Its coverage, routable starts, graph provenance, immutable
map-data version, and reviewed workload values come from one fail-closed production manifest.

```powershell
$env:GPSART_DATABASE_URL = $env:DATABASE_URL
$env:GPSART_API_BIND = "127.0.0.1:8080"
$env:GPSART_PUBLIC_ORIGIN = "https://gpsart.localhost"
$env:GPSART_ROUTING_MANIFEST = "ops/routing/production-pilot-v1.json"
$env:RUST_LOG = "info"
```

The checked-in manifest records the bounded Bengaluru pilot and explicitly reviews walking and
running starts only. Startup rejects missing or malformed manifests, graph/checksum/provenance
mismatches, unknown versions, unreviewed profiles, invalid starts, and runtime workload overrides
that differ from the approved values. Follow the recorded update runbook rather than editing an
existing immutable manifest or graph.

Start the API in one terminal:

```powershell
cargo run -p gpsart-api
```

Start the worker in another terminal if you want to exercise its readiness/reconnect behavior:

```powershell
$env:GPSART_DATABASE_URL = "postgres://postgres:change-me@127.0.0.1:5432/gpsart"
$env:GPSART_COVERAGE_VERSION = "bengaluru-central-pilot-v1"
$env:GPSART_ROUTING_GRAPH_PATH = "$PWD/crates/gpsart-routing/data/bengaluru-central-osm-v3.json"
$env:GPSART_ROUTING_GRAPH_SHA256_PATH = "$PWD/crates/gpsart-routing/data/bengaluru-central-osm-v3.sha256"
$env:RUST_LOG = "info"
cargo run -p gpsart-worker
```

The worker fails closed if its graph, checksum, coverage version, or database configuration is
missing or inconsistent. A non-secret deployment example is tracked in `ops/worker.env.example`.

### 4. Serve the built client and provide same-origin HTTPS

Run Vite's production-file preview on its default port:

```sh
npm run preview
```

For a local Caddy installation, save this as a temporary `Caddyfile` outside the repository and run
`caddy run --config <path-to-Caddyfile>`:

```caddyfile
https://gpsart.localhost {
    handle /api/* {
        reverse_proxy 127.0.0.1:8080
    }

    handle {
        reverse_proxy 127.0.0.1:4173
    }
}
```

Open `https://gpsart.localhost`. Trust Caddy's local CA when prompted. The browser URL must match
`GPSART_PUBLIC_ORIGIN` exactly, including scheme and port if a non-default port is used. A plain HTTP
origin will not work because the API intentionally issues Secure cookies.

### 5. Smoke-check the stack

Through the reverse proxy:

```sh
curl -fsS https://gpsart.localhost/health/live
curl -fsS https://gpsart.localhost/health/ready
curl -fsS https://gpsart.localhost/api/v1/designs
```

Expected health responses use `status: "ok"`. Readiness fails closed when PostgreSQL is unavailable
or migration state is not current. In the browser, verify that the catalogue loads and that a guest
session creates `gpsart_guest` (HttpOnly) and `gpsart_csrf` Secure cookies.

## Configuration reference

Required API variables:

| Variable | Purpose | Example/format |
| --- | --- | --- |
| `GPSART_DATABASE_URL` | PostgreSQL connection; treated as a secret | `postgres://user:password@db/gpsart` |
| `GPSART_API_BIND` | API listen address | `0.0.0.0:8080` |
| `GPSART_PUBLIC_ORIGIN` | Exact canonical HTTPS browser origin used by CSRF policy | `https://gpsart.example.com` |
| `GPSART_ROUTING_MANIFEST` | Approved immutable graph, coverage, start, and workload configuration | `ops/routing/production-pilot-v1.json` |

The worker requires only `GPSART_DATABASE_URL`. Both services accept `RUST_LOG`; structured JSON logs
default to `info`.

Optional workload variables and their defaults are accepted only when they equal the values in the
approved production manifest; publish a reviewed successor manifest to change them:

| Variable | Default | Constraint |
| --- | ---: | --- |
| `GPSART_MAX_DISTANCE_METRES` | `100000` | positive |
| `GPSART_MAX_TEMPLATE_POINTS` | `512` | positive |
| `GPSART_MAX_ANCHORS` | `64` | positive |
| `GPSART_MAX_CANDIDATE_ATTEMPTS` | `3` | positive |
| `GPSART_MAX_PROVIDER_REQUESTS` | `50` | positive |
| `GPSART_MAX_REQUEST_BYTES` | `65536` | positive |
| `GPSART_MAX_RESPONSE_BYTES` | `2097152` | positive |
| `GPSART_MAX_RUNTIME_SECONDS` | `60` | positive and no more than 60 |
| `GPSART_MAX_ACTIVE_JOBS_PER_GUEST` | `2` | positive and no more than deployment cap |
| `GPSART_MAX_ACTIVE_JOBS_DEPLOYMENT` | `20` | positive |
| `GPSART_MAX_SUBMISSIONS_PER_GUEST_PER_MINUTE` | `10` | positive |
| `GPSART_GUEST_RETENTION_SECONDS` | `86400` | positive and no more than 24 hours |

Unsupported values fail startup; the service never silently widens these limits.

## Production deployment reference

The supported production target is Kubernetes on `linux/amd64`. Digest-pinned OCI definitions,
separate staging/production inputs, the complete Kubernetes stack, delivery/rollback workflows, and
the clean-environment procedure are checked in under `deploy/`, `.github/workflows/`, and
[`docs/runbooks/production-deployment.md`](docs/runbooks/production-deployment.md). The Sites package
remains a static preview and is not the production stack.

### Build immutable artifacts

```sh
npm ci
npm run build
cargo build --workspace --release --locked
```

The delivery workflow builds and deploys:

- `dist/client/` as immutable static files;
- `target/release/gpsart-api` as the API service;
- `target/release/gpsart-worker` with its reviewed checksum-pinned graph and non-secret settings;
- a backed-up PostgreSQL/PostGIS database with migrations applied as a separate release step.

Local Windows builds are for development only. Release images are built by the controlled Linux
Buildx runner for `linux/amd64`, recorded by digest, and promoted unchanged from staging.

### Route traffic

TLS ingress sends the entire canonical origin to `gpsart-client-edge`. Its checked-in nginx policy
serves immutable client assets and proxies `/api/*` and `/health/*` internally, with bounded bodies
and timeouts, security headers, and privacy-safe access logs.

Set `GPSART_PUBLIC_ORIGIN=https://gpsart.example.com`. Keep API health endpoints on an internal
monitoring path or deliberately expose only the information already returned by them. Configure
graceful shutdown so SIGTERM/interrupt reaches the Rust processes; the API drains the server and
stops its cleanup task, while the worker exits its loop.

### Database release order

1. Back up the database using encrypted, access-audited storage.
2. Run `cargo sqlx migrate info --source migrations` against the target.
3. Apply `cargo run -p gpsart-persistence --bin gpsart-migrate` as a separately authorized release job.
4. Start the API and verify `/health/ready` before shifting traffic.
5. Start workers only after the API/database version is confirmed compatible.
6. Roll back application binaries if necessary; database migrations are forward-only. Do not edit or
   delete an applied migration.

### Mandatory production controls

- Store `GPSART_DATABASE_URL` in a secret manager; never bake it into images or client bundles.
- Disable proxy logging of query strings, bodies, Cookie/authorization/CSRF headers, exact
  coordinates, and route geometry.
- Encrypt backups, audit access, retain them for no more than seven days, and run expired-data cleanup
  after any restore before serving traffic.
- Preserve visible OpenStreetMap attribution. Review ODbL obligations before publishing a derived
  routing database.
- Do not bulk-fetch or prefetch `tile.openstreetmap.org`; production use must satisfy the provider
  review in [`docs/release/provider-obligations.md`](docs/release/provider-obligations.md).
- Import the dashboard and alerts from `ops/observability/` into a compatible monitoring system and
  follow [`docs/runbooks/observability-alerts.md`](docs/runbooks/observability-alerts.md).
- Keep exact coordinates, traces, credentials, cookies, tokens, and custom text out of telemetry and
  support tickets. See [`docs/privacy-retention-policy.md`](docs/privacy-retention-policy.md).
- Complete every blocked release gate and record a new signed `go` decision before public launch.

## Features and operating boundaries

| Capability | Status in this revision |
| --- | --- |
| Responsive planner, map selection, browser geolocation | Implemented |
| Curated design catalogue | Served by the API |
| Private guest session, CSRF, rotation, revocation | Implemented; HTTPS required |
| Start coverage, snap proposal, explicit confirmation | Implemented from configured reviewed points |
| Durable job submit/status/cancel | Implemented |
| Worker route generation and candidate publication | Implemented and exercised through the connected PostgreSQL-backed vertical slice |
| Candidate comparison/details | Implemented for authorized server snapshots |
| GPX 1.1 export | Implemented for a real loaded snapshot |
| Save/rename/delete routes | Implemented in browser IndexedDB only |
| Google sign-in | Design-only, not connected |
| Place-name search/geocoding | Not connected; use coordinates, map click, or geolocation |
| Geography | Pinned Bengaluru pilot data only; arbitrary-start feasibility is not promised |

The bundled pilot graph is deliberately small and is not a Bengaluru-wide extract. Read
[`crates/gpsart-routing/data/README.md`](crates/gpsart-routing/data/README.md) before replacing or
publishing routing data.

## Verification

Run the complete repository gate before review:

```sh
npm ci
npm run factory:verify
```

Useful focused commands:

```sh
npm run typecheck
npm run test:unit
npm run build
npm run test:sites
npm run test:visual-shell
npm run test:a11y
npm run rust:fmt
npm run rust:clippy
npm run rust:test
```

`npm run test:a11y` needs Chrome/Chromium. If it is not auto-detected, set
`GPSART_CHROME_PATH` to the executable. PostgreSQL integration tests use `DATABASE_URL` (not
`GPSART_DATABASE_URL`). Older component suites explicitly skip their database cases when it is
absent, but `npm run test:connected` is a required release-path check and fails when it is absent.
Point `DATABASE_URL` at a disposable administrative PostGIS database; the tests create and drop
isolated databases.

## Repository map

- `src/` — TypeScript client, application state, and browser adapters
- `crates/gpsart-api/` — HTTP API and authorization boundary
- `crates/gpsart-worker/` — worker lifecycle and provider-budget primitives
- `crates/gpsart-routing/` — pinned graph, route construction, and quality checks
- `crates/gpsart-persistence/` — PostgreSQL stores, readiness, and migration integration
- `migrations/` — forward-only SQLx migrations
- `openapi/gpsart.yaml` — public HTTP contract
- `ops/` — alert/dashboard and workload definitions
- `docs/` — policies, ADRs, evidence, limitations, and operational runbooks
- `factory/` — story state, handoffs, and code-factory checks

## Contribution workflow

All contributors and agents must follow [`AGENTS.md`](AGENTS.md) and
[`factory/guardrails.md`](factory/guardrails.md). Inspect the story graph with
`npm run factory:status`, claim only work returned by `npm run factory:next -- --role dev`, add a
failing test before behavior changes, run `npm run factory:verify`, and obtain independent QA before
marking implementation complete.
