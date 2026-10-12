# GPSART technical delivery backlog

Status: production-readiness backlog active<br>
Prepared: 2 October 2026; production stages added 12 October 2026<br>
Source: [Web App Epic for Routes That Draw Shapes and Text](problem-statement.md)

This backlog translates the product stories and interactive prototype into engineering work. It is ordered to prove route feasibility before investing heavily in the production UI. The acceptance criteria below are technical evidence; the corresponding product-story acceptance criteria still apply.

Do not assign a delivery date or a single epic estimate until the routing-feasibility gate in Stage 1 has passed. The number of provider calls, required graph control, search latency, and achievable shape quality are not yet measured.

## Architecture baseline

These are implementation decisions for the first release.

| Area | Decision | Reason |
| --- | --- | --- |
| Backend | Rust workspace using Tokio, Axum, Serde, and SQLx | One language and shared domain types for the API, worker, validation, and fitting engine; async I/O for job and routing calls |
| Processes | One codebase with separate `api` and `worker` binaries | Independent scaling and failure isolation without premature microservices |
| Frontend | Native HTML, CSS, and TypeScript modules built by Vite; no component framework | Preserves the prototype direction and keeps the shipped client small. Leaflet remains the focused map library |
| API contract | Versioned REST/JSON under `/api/v1`; GeoJSON for geometry; checked-in OpenAPI contract | Stable client/server boundary and explicit longitude/latitude ordering |
| Database | PostgreSQL with PostGIS, managed by SQLx migrations | Required for durable jobs, leases, idempotency, session ownership, expiry, and spatially indexed route data |
| Job queue | PostgreSQL queue using atomic claims, `FOR UPDATE SKIP LOCKED`, and expiring leases | Meets pilot needs without adding Redis or a message broker |
| Routing | A provider-neutral Rust adapter, with self-hosted Valhalla as the first pilot candidate | Keeps provider policy outside the domain while testing walking, cycling, turn restrictions, ordered anchors, and local pilot coverage. Adoption is conditional on TS-104 passing |
| Templates | Versioned, reviewed JSON assets in source control | Ordered strokes and orientation rules are product data that need reviewable history and deterministic tests |
| Browser persistence | IndexedDB containing a versioned complete candidate snapshot | Route geometry can exceed practical `localStorage` limits and must reopen/export without a live server job |
| Client job updates | Poll with bounded backoff for the pilot | Simple recovery after refresh; SSE can be added later only if polling load justifies it |
| Authentication | Opaque guest session in a secure, HTTP-only, SameSite cookie | P0 planning has no sign-up. Google sign-in stays outside the production path until the P1 cloud-save story |
| Deployment | Containerized API and worker plus managed PostgreSQL/PostGIS; static frontend served separately or by the API edge | Reproducible runtime with independently scalable compute and static delivery |

The routing adapter is deliberately a boundary, not an assertion that a conventional directions API can solve design fitting. If TS-104 shows that ordered anchors and provider controls cannot produce legal, recognisable candidates within the budget, the replacement is a locally queryable routing graph behind the same interface.

### Proposed repository shape

```text
/
  Cargo.toml
  crates/
    gpsart-api/
    gpsart-domain/
    gpsart-persistence/
    gpsart-routing/
    gpsart-worker/
  migrations/
  openapi/
    gpsart-v1.yaml
  templates/
  fixtures/
    routing/
  src/                  # native browser TypeScript and CSS
  tests/
    e2e/
    contract/
  docs/
```

The existing HTML/CSS prototype remains the visual reference while production modules replace simulated state incrementally.

## Delivery map

| Stage | Exit outcome | Stories |
| --- | --- | --- |
| 0. Engineering foundation | Rust services, native TypeScript client, contract checks, and PostGIS run locally and in CI | TS-001–TS-003 |
| 1. Routing feasibility | One shape and one curated text design produce legal full traversals on controlled and pilot-area fixtures | TS-101–TS-107 |
| 2. Real vertical slice | A guest creates a real job, receives a route, inspects it, and exports the same traversal | TS-201–TS-207 |
| 3. First-release planner | The full P0 catalogue, comparison, recovery, local save, responsive, and accessible flows work | TS-301–TS-305 |
| 4. Pilot hardening | Privacy, limits, recovery, performance, provider obligations, and release evidence meet the epic gates | TS-401–TS-404 |
| 5. Production implementation | The deployed stack generates real routes using reviewed data and has deploy, recovery, privacy, provider, and operational controls | TS-501–TS-509 |
| 6. Independent release validation | Held-out quality and real-browser gates pass and an accountable release decision is signed | TS-601–TS-603 |

`Blocked by` names technical prerequisites, not every related product story. Work within a stage can proceed in parallel after its prerequisites are satisfied.

## Stage 0 — engineering foundation

### TS-001 — Bootstrap the Rust application workspace

**Priority:** P0<br>
**Maps to:** US05, US14<br>
**Blocked by:** none

Create a Rust workspace with shared domain, API, persistence, routing, and worker crates. Provide separate API and worker binaries, centralized configuration, structured errors, tracing initialization, and graceful shutdown.

Acceptance criteria:

1. `cargo build --workspace` and `cargo test --workspace` succeed from a clean checkout.
2. `gpsart-api` exposes `/health/live` and `/health/ready`; readiness fails when PostgreSQL is unavailable.
3. `gpsart-worker` starts independently, checks its dependencies, and stops taking work after a shutdown signal.
4. Configuration comes from environment variables with startup validation. Secrets and provider credentials have no committed defaults and are redacted from logs.
5. Domain types do not depend on Axum, SQLx, or a particular routing provider.
6. CI runs formatting, Clippy with warnings denied, workspace tests, frontend checks, and secret scanning.

Evidence: CI run plus a local start/stop smoke test for both binaries.

### TS-002 — Establish PostgreSQL/PostGIS persistence and migrations

**Priority:** P0<br>
**Maps to:** US05, US09, US14<br>
**Blocked by:** TS-001

Create forward-only SQLx migrations for guest sessions, resolved starts, immutable planning requests, generation jobs, job attempts, route candidates, and cleanup metadata. Enable PostGIS and use UTC timestamps and UUID identifiers.

Acceptance criteria:

1. A clean PostgreSQL instance can apply every migration, and the application refuses to run with unapplied or incompatible migrations.
2. The job state is constrained to `queued`, `running`, `succeeded`, `no_match`, `failed`, `cancelled`, or `timed_out`.
3. The schema enforces one terminal outcome, request ownership, immutable candidate versions, and uniqueness of `(guest_session_id, idempotency_key)`.
4. Requested pins, confirmed starts, endpoints, and canonical route geometries use SRID 4326 and have documented GeoJSON conversion.
5. A request snapshot stores metre-based inputs and all route-affecting profile, template, map-data, and algorithm versions.
6. Migration tests cover a clean install and upgrade from the previous checked-in migration.

Evidence: migration test output and an entity/retention diagram checked into `docs/`.

### TS-003 — Convert the prototype shell to native TypeScript modules

**Priority:** P0<br>
**Maps to:** US01–US13<br>
**Blocked by:** none

Keep the existing visual design while separating rendering, router, state, map, API, storage, and screen controllers into typed browser modules. Remove the unused React packages and do not introduce a component framework.

Acceptance criteria:

1. Vite performs strict TypeScript checking and builds all current HTML entry points without React or React DOM.
2. The planner, catalogue, generating, results, details, saved, no-match, coverage, error, and account design states remain reachable for review.
3. Route-affecting form state has one typed model; screen code cannot create ad hoc candidate geometry or success metrics.
4. API responses are decoded and validated at the client boundary before entering application state.
5. Existing browser-storage records either migrate to the new schema or receive a clear unsupported-snapshot state; malformed records do not break app startup.
6. Unit tests cover request revision changes, distance-bound calculation, stale-result rejection, and route snapshot decoding.

Evidence: production build, unit-test output, and screenshots of the retained desktop and 360 px prototype flows.

## Stage 1 — routing feasibility

### TS-101 — Define and validate versioned design templates

**Priority:** P0<br>
**Maps to:** US03, US06<br>
**Blocked by:** TS-001

Define a source-controlled template schema for normalized ordered strokes, joins, permitted direction, scale/aspect bounds, orientation rules, identifying features, and complexity limits. Seed `heart` and `HI` as the feasibility pair.

Acceptance criteria:

1. Templates have stable IDs and monotonically versioned immutable definitions.
2. The loader rejects empty strokes, non-finite coordinates, invalid joins, unsupported orientation rules, and excessive complexity.
3. Text templates cannot be mirrored or reordered and are limited to plus or minus 15 degrees on a north-up map.
4. Multi-stroke templates explicitly classify inter-stroke connectors and retracing; no travel can be omitted from the planned traversal.
5. Golden tests render the normalized template and verify stable hashes for published versions.
6. Only templates with `published` status appear through the catalogue API.

Evidence: reviewed heart and HI template files, rendered fixtures, and schema-validation tests.

### TS-102 — Model activity profiles and conservative access policy

**Priority:** P0<br>
**Maps to:** US02, US06<br>
**Blocked by:** TS-001

Create versioned walking, running, and cycling profiles with access, direction, turn, surface, step, ferry, motorway, and missing-metadata policies. Walking and running may share access rules but retain separate pace defaults.

Acceptance criteria:

1. Every route request and candidate records the exact profile version used.
2. Cycling rejects steps, motorways, motorroad edges, ferries, and profile-forbidden access in controlled fixtures.
3. Walking/running and cycling produce different eligibility on a fixture containing a foot-only edge.
4. Missing surface, elevation, or access metadata remains `unknown`; it is never converted to zero, paved, or verified access.
5. Pace assumptions affect only duration estimates and never route legality or canonical distance.

Evidence: table-driven tests for each access rule and a human-readable profile policy document.

### TS-103 — Implement coverage and routable-start resolution

**Priority:** P0<br>
**Maps to:** US01, US08<br>
**Blocked by:** TS-002, TS-102

Implement configurable pilot coverage and a profile-aware start resolver that keeps the requested pin distinct from a proposed routable point.

Acceptance criteria:

1. `GET /api/v1/coverage` returns versioned GeoJSON coverage without claiming local route feasibility.
2. `POST /api/v1/starts/resolve` returns the requested point, candidate network point, snap distance in metres, profile version, data version, expiry, and confirmation requirement.
3. A snap of 30 metres or less can be explicitly confirmed; a greater displacement cannot be submitted as an accepted start.
4. Unsupported coverage, no routable access, provider failure, invalid coordinates, and expired resolutions have distinct machine-readable outcomes.
5. A resolution is private to its guest session and invalid after its profile, coverage policy, data version, or expiry changes.
6. Exact coordinates are absent from request logs, analytics, and public URLs.

Evidence: API contract tests plus fixtures for inside coverage, outside coverage, short snap, long snap, and no access.

### TS-104 — Prove the routing-adapter capability

**Priority:** P0 / feasibility gate<br>
**Maps to:** US06, US14<br>
**Blocked by:** TS-102

Implement a provider-neutral routing trait and adapters for an in-memory controlled graph and the proposed pilot routing engine. Time-box a capability spike against a pinned Bengaluru data snapshot before adopting the provider.

Acceptance criteria:

1. The adapter accepts an activity profile plus ordered anchors and returns ordered edges, geometry, distance, manoeuvres, and available provenance.
2. Controlled-graph tests prove one-way, forbidden-turn, activity access, repeated-edge, disconnected-component, and grade-separated-crossing behavior.
3. Cancellation and a per-call deadline propagate through the adapter; retries cannot exceed the job budget.
4. The pilot adapter demonstrates legal paths for walking and cycling, repeated use of an edge, and at least one multi-anchor heart and HI attempt.
5. Provider limits, latency, attribution, caching, retention, deployment cost, and waypoint/control constraints are recorded from the actual setup.
6. The story ends with a recorded adopt/replace decision. Failure to control the necessary traversal blocks TS-105 and triggers evaluation of a locally queryable graph behind the same trait.

Evidence: controlled-graph suite, pilot spike report, captured route fixtures, and architecture decision record.

### TS-105 — Generate a legal heart-route candidate

**Priority:** P0 / feasibility gate<br>
**Maps to:** US06<br>
**Blocked by:** TS-101–TS-104

Implement a bounded fitting search for the heart template: local metric projection, placement/scale/rotation variants, network anchors, ordered routing, connector/closure assembly, and hard validation.

Acceptance criteria:

1. The requested confirmed start is the first traversal point; loop candidates end on the same network point within 5 metres without a synthetic closing line.
2. The full ordered route contains access, artwork, connector, retraced, and return traversal occurrences with no hidden geometry.
3. Total distance is computed from ordered routed traversal in metres, counts repeated travel each time, and is inside the unrounded request interval.
4. Any illegal edge, turn discontinuity, disconnected leg, out-of-range distance, or deadline-exhausted incomplete candidate is rejected before scoring.
5. Search limits placement count, scale/rotation variants, anchors, provider calls, memory, and wall time from configuration.
6. A known feasible controlled fixture produces a recognisable legal result and a negative fixture returns no candidate rather than a straight-line fallback.

Evidence: deterministic fixture result, validation trace, and rendered full-route preview reviewed against the heart template.

### TS-106 — Generate a continuous curated-text candidate

**Priority:** P0 / feasibility gate<br>
**Maps to:** US03, US06<br>
**Blocked by:** TS-105

Extend fitting to the HI template while preserving stroke and letter order, upright orientation, explicit connectors, and retracing.

Acceptance criteria:

1. All letter strokes and connectors are included in one continuous ordered traversal and total distance.
2. Mirrored, reordered, disallowed-rotation, disconnected, or visually incomplete candidates fail hard validation.
3. Connector and repeated distances are calculated by traversal occurrence and remain diagnostics included in total distance, not extra totals.
4. The canonical full trace, not an ideal outline or artwork-only subset, is used for resemblance assessment.
5. A known feasible text fixture produces a legal result; a disconnected fixture returns a classified no-match outcome.

Evidence: rendered full traces with connector classification plus deterministic positive and negative fixture tests.

### TS-107 — Score, deduplicate, and benchmark candidates

**Priority:** P0 / feasibility gate<br>
**Maps to:** US06, US07; routing benchmark gate<br>
**Blocked by:** TS-105, TS-106

Implement versioned quality metrics after hard validation, deterministic ranking, geometric/traversal deduplication, and a repeatable benchmark harness.

Acceptance criteria:

1. Scoring records full-trace error, distance error, repeated ratio, connector ratio, manoeuvre complexity, identifying-feature checks, and the weights/version used.
2. Distance and legality remain hard gates and cannot be offset by a better resemblance score.
3. Exact traversal duplicates and point-density-only variants collapse to one result; genuinely distinct legal traversals remain eligible.
4. User-facing output is `strong_match` or `approximate_match`; uncalibrated numeric scores are not exposed as percentages.
5. The harness pins map, profile, template, and algorithm versions and records latency, provider calls, outcome, validity, and route artifacts.
6. Stage 1 cannot exit without at least one reviewed shape and one reviewed text traversal on pilot-area data. The final release dataset must later reach the epic's 50 feasible and 20 difficult/no-match cases.

Evidence: benchmark report and machine-readable results committed without private user coordinates.

## Stage 2 — real vertical slice

### TS-201 — Create private guest sessions and request authorization

**Priority:** P0<br>
**Maps to:** US14<br>
**Blocked by:** TS-001, TS-002

Issue an opaque guest-session cookie and authorize every resolved start, job, and candidate through its owner rather than by identifier secrecy.

Acceptance criteria:

1. The cookie is random, Secure in deployed environments, HTTP-only, SameSite=Lax or stricter, rotated under a documented policy, and stores no coordinates.
2. Only a one-way token digest is persisted; session comparison is constant-time at the authentication boundary.
3. State-changing cookie-authenticated requests enforce origin/CSRF protections.
4. Another guest receives a non-disclosing denial when requesting, cancelling, or exporting a foreign job or route ID.
5. Expired sessions cannot start work; cleanup does not log or expose their route geometry.

Evidence: integration tests with two isolated clients and cookie/security-header inspection.

### TS-202 — Implement idempotent route-job creation and durable worker leases

**Priority:** P0<br>
**Maps to:** US05, US09, US14<br>
**Blocked by:** TS-002, TS-103, TS-201

Implement job creation, atomic worker claims, renewable leases, bounded attempts, cancellation, deadlines, and terminal-state publication in PostgreSQL.

Acceptance criteria:

1. `POST /api/v1/route-jobs` validates the complete request and returns `202` with a job ID for an owned, current start resolution.
2. Repeating the same idempotency key and canonical input returns the original job; the same key with different input returns `409`.
3. Concurrent workers cannot own one valid lease at the same time. Expired leases are recovered without publishing duplicate candidates.
4. Cancellation is idempotent and wins atomically against late completion unless a terminal result was already committed.
5. The worker never schedules a provider call after cancellation or after the remaining job budget is insufficient.
6. At the deadline, fully validated candidates may publish with a budget notice; otherwise the result is `timed_out`, not a false `no_match` or success.

Evidence: concurrency tests exercising duplicate submission, lease expiry, cancellation race, crash recovery, and timeout.

### TS-203 — Expose truthful job state and classified outcomes

**Priority:** P0<br>
**Maps to:** US05, US08<br>
**Blocked by:** TS-202

Implement the authorized job-status contract and outcome taxonomy used by the prototype's generating and failure screens.

Acceptance criteria:

1. `GET /api/v1/route-jobs/{id}` returns state, truthful coarse stage, timestamps, expiry, outcome/error class, result IDs, request revision, and retry guidance where appropriate.
2. Stages are limited to observed work such as `checking_roads`, `fitting_design`, and `comparing_routes`; no invented completion percentage is returned.
3. Outcomes distinguish unsupported coverage, no routable access, disconnected network, insufficient visual match, outside distance, provider unavailable, exhausted budget, cancellation, and timeout when evidence supports it.
4. Poll responses support conditional requests or a monotonic version so unchanged polling is inexpensive.
5. Terminal state and referenced result IDs are committed atomically.

Evidence: state-transition tests and examples for every terminal response in the OpenAPI contract.

### TS-204 — Connect the native planner to start resolution and jobs

**Priority:** P0<br>
**Maps to:** US01–US05, US09<br>
**Blocked by:** TS-003, TS-103, TS-201–TS-203

Replace simulated generation with API-backed start confirmation, request submission, polling, cancellation, refresh recovery, and stale-result protection.

Acceptance criteria:

1. Search/manual coordinates, map selection, and on-demand browser geolocation all produce the same typed proposed-start model.
2. Browser geolocation is requested only after the user action; denial, timeout, unsupported access, and low accuracy leave manual entry available.
3. A job cannot start until required start confirmation succeeds and all fields pass client and server validation.
4. Double submission reuses one idempotency key while the logical request is unchanged.
5. Refresh resumes an owned, unexpired active job; polling uses bounded backoff and stops at a terminal outcome or cancellation.
6. Every route-affecting edit increments the request revision. A response for an older revision is retained as a previous result and cannot replace the current plan.

Evidence: browser tests for submit, refresh, cancel, late response, invalid fields, denied geolocation, and unsupported coverage.

### TS-205 — Store and serve immutable route-candidate snapshots

**Priority:** P0<br>
**Maps to:** US07, US10<br>
**Blocked by:** TS-107, TS-202

Persist the exact validated route result and expose it without recalculation through `GET /api/v1/routes/{id}`.

Acceptance criteria:

1. A candidate stores one immutable route version with ordered GeoJSON, traversal/leg classifications, instructions, start/finish, distances, quality band, qualifications, and provenance.
2. Artwork distance plus connector distance equals total distance within an explicit numeric tolerance; repeated distance is marked as included rather than added again.
3. Map geometry, instructions, direction markers, duration, and summaries derive from that same ordered snapshot.
4. Unknown surface, access, elevation, and provider metadata serialize as unknown/absent, never invented values.
5. Candidate responses include request, profile, template, algorithm, routing-data, and candidate versions plus generation time.
6. Access requires the owning guest session even after the source job reaches a terminal state.

Evidence: invariant/property tests and an API-to-map snapshot test at a self-intersection.

### TS-206 — Render real comparison and route details

**Priority:** P0<br>
**Maps to:** US07, US10<br>
**Blocked by:** TS-003, TS-205

Bind real candidate snapshots to the existing results and details designs, including the complete path, direction, connectors, repeated distance, endpoint policy, and provenance.

Acceptance criteria:

1. Results show one to three distinct candidates without placeholder duplication.
2. Selecting a card changes the highlighted full geometry and direction while preserving a north-up comparison convention.
3. Labels such as Best shape match or Closest distance are computed from the displayed set and its metrics.
4. Detail metrics and text alternatives remain usable when the base map fails to load.
5. The UI never substitutes the ideal design outline for routed geometry and clearly labels optional overlays.

Evidence: end-to-end tests with one, two, and three candidates plus a base-map failure case.

### TS-207 — Export the exact candidate as GPX 1.1

**Priority:** P0<br>
**Maps to:** US11<br>
**Blocked by:** TS-205

Implement authorized server export and an offline browser export path from a complete local snapshot. Both serialize the stored traversal without rerouting it.

Acceptance criteria:

1. `GET /api/v1/routes/{id}/export.gpx` returns valid GPX 1.1 with all ordered points, connectors, retraced sections, and planned-route metadata.
2. Export contains no fabricated activity timestamps, speed, heart rate, or completion claims.
3. Filename components are predictable and sanitized; response headers prevent content-type confusion.
4. Reimport reproduces order and geometry within the documented serialization tolerance and preserves self-intersection traversal order.
5. Server and offline exports from the same candidate snapshot are geometrically equivalent.
6. A checked external importer opens the pilot fixture before release; untested compatibility is not claimed.

Evidence: schema validation, round-trip property tests, and recorded external-importer check.

## Stage 3 — first-release planner

### TS-301 — Publish the curated first-release catalogue

**Priority:** P0<br>
**Maps to:** US03, US06<br>
**Blocked by:** TS-101, TS-107

Add reviewed square, circle, triangle, star, GO, and RUN templates only when each passes schema, controlled-fixture, and recognisability review. Heart and at least one text template remain required even if complex candidates are deferred.

Acceptance criteria:

1. `GET /api/v1/designs` supports category filtering and returns accessible name, thumbnail/preview data, complexity hint, status, and template version.
2. Every published template has at least one legal positive fixture and relevant negative fixtures.
3. A disabled template explains its status without clearing the planner form.
4. Catalogue presence never claims feasibility at the current location.
5. The UI has accessible selected state and preserves start, activity, and distance when the design changes.

Evidence: catalogue contract test, fixture matrix, and template review record.

### TS-302 — Complete no-match, error, and refinement flows

**Priority:** P0<br>
**Maps to:** US08, US09<br>
**Blocked by:** TS-203, TS-204

Connect classified outcomes to the prototype's coverage, no-match, and dependency-error screens and support explicit request revisions.

Acceptance criteria:

1. Copy states that a bounded search found no qualifying result and never claims impossibility without evidence.
2. Suggestions are driven by the outcome class but are not marked feasible unless a feasibility check was completed.
3. Widening tolerance, changing distance/start/design/profile, or toggling return-to-start happens only after user action and creates a new revision.
4. The original request summary and prior result remain viewable with their original inputs.
5. Retrying a dependency failure creates or resumes work according to the idempotency contract and never duplicates a completed job.

Evidence: end-to-end tests for every failure screen and a refine-after-success/no-match flow.

### TS-303 — Save complete route snapshots on the device

**Priority:** P0<br>
**Maps to:** US12<br>
**Blocked by:** TS-003, TS-205, TS-207

Replace localStorage saving with an IndexedDB repository containing a versioned complete request and candidate snapshot.

Acceptance criteria:

1. Save, reopen, rename, delete, list, and offline GPX export work without an account.
2. Reopening uses saved geometry and metadata without rerouting or requiring an unexpired server job.
3. The UI consistently says Saved on this device and explains the effect of clearing site data.
4. Transactions prevent a failed rename/delete from corrupting another record.
5. Storage denial, quota exhaustion, malformed data, and unknown schema versions produce recoverable per-record errors and keep the selected server route exportable when available.
6. Signing in or out does not upload, erase, or relabel local routes.

Evidence: browser storage tests including offline reopen/export and injected quota/corruption failures.

### TS-304 — Meet responsive and accessible journey requirements

**Priority:** P0<br>
**Maps to:** US13<br>
**Blocked by:** TS-204, TS-206, TS-303

Harden the complete planner-to-export flow for keyboard, screen-reader, touch, reduced-motion, and small-screen use without changing the approved visual language.

Acceptance criteria:

1. The flow works at 360 px, 768 px, and desktop widths with no page-level horizontal overflow or unreachable primary action.
2. Inputs, tabs, template items, result cards, dialogs, errors, map alternatives, and progress have correct names, roles, focus order, and visible focus.
3. Field errors are associated with fields; selected/error state does not rely on color alone.
4. Dialogs restore focus, mobile panels do not trap focus, and screen updates do not repeatedly interrupt assistive technology.
5. Text start entry and route instructions provide an equivalent path through key tasks without precise map interaction.
6. Automated accessibility checks have no serious/critical findings, followed by documented keyboard and screen-reader smoke tests.

Evidence: viewport screenshots, automated report, and manual test checklist.

### TS-305 — Remove prototype-only success paths

**Priority:** P0<br>
**Maps to:** epic definition of done<br>
**Blocked by:** TS-301–TS-304

Prevent illustrative data from appearing in production builds while keeping an explicit design-review mode for the prototype screens.

Acceptance criteria:

1. Production code contains no timer-created success, hard-coded route candidates, fabricated routing metrics, or sample GPX action.
2. A production route result can enter state only after API decoding, authorization, and required invariant checks.
3. Design-review fixtures are isolated behind a build-time development flag and visibly marked; the flag cannot be enabled by a public query parameter.
4. CI scans the production bundle for known prototype labels and candidate fixture IDs.
5. The production empty, loading, no-match, dependency-error, and unavailable-map states work without fixture data.

Evidence: bundle check and end-to-end test using only the real API test environment.

## Stage 4 — pilot hardening

### TS-401 — Enforce workload, retention, and privacy limits

**Priority:** P0<br>
**Maps to:** US14<br>
**Blocked by:** TS-202, TS-205

Apply server-side caps and cleanup for guest work and ensure precise location data stays out of logs and analytics.

Acceptance criteria:

1. Limits cover distance, tolerance choices, template complexity, anchors, candidate attempts, provider requests, response size, runtime, and concurrent jobs per guest and deployment.
2. Rate limits return `429` with retry guidance and cannot be bypassed by choosing new idempotency keys in one guest session.
3. Unsaved guest jobs and candidates expire after the configured default of 24 hours; cleanup is batched, retryable, and tested around active leases.
4. Application, proxy, error, and analytics logs exclude exact start points, full traces, cookie values, and future custom text.
5. Provider credentials remain server-side; intentionally public map credentials are origin-restricted.
6. Backup and deletion behavior for precise route records is documented and matches the retention policy.

Evidence: abuse/limit tests, cleanup test, and structured-log privacy audit.

### TS-402 — Add privacy-preserving observability

**Priority:** P0<br>
**Maps to:** US14 and product metrics<br>
**Blocked by:** TS-202, TS-401

Instrument the API, worker, database queue, fitting engine, and provider adapter without emitting route geometry.

Acceptance criteria:

1. Correlation IDs connect request, job, attempts, provider calls, and terminal outcomes without using a user or coordinate as the identifier.
2. Metrics include acknowledgement latency, queue delay, search latency, terminal outcome, cancellation/timeout, provider calls, candidates returned, activity, distance bucket, and shape/text category.
3. Logs record profile, template, map-data, and algorithm versions plus timing and result class, but no precise route data.
4. Alerts cover queue backlog, stuck leases, provider failure rate, error rate, timeout rate, and cleanup failure.
5. A redaction test fails CI if protected field names or representative coordinate payloads reach logging sinks.

Evidence: sample dashboard/queries, alert test, and redaction-test output.

### TS-403 — Verify recovery and pilot performance

**Priority:** P0<br>
**Maps to:** nonfunctional requirements and US14<br>
**Blocked by:** TS-107, TS-202, TS-402

Exercise the documented pilot workload and failure modes on recorded hardware and the selected provider setup.

Acceptance criteria:

1. Load tests cover 20 concurrent active jobs with representative activity, distance, shape, and text inputs.
2. Reports include p50/p95/p99 acknowledgement, queue, provider, and total latency plus result quality, no-match, timeout, cancellation, and provider-call distributions.
3. The target is 95% accepted-job acknowledgement within 1 second and 95% benchmark termination within 30 seconds with a 60-second maximum; misses are recorded as release decisions, not hidden by averages.
4. Worker termination during fitting, database reconnect, provider timeout/quota, and lease expiry recover without duplicate publication or false success.
5. A slow or absent base map does not lose form state or generated route data.

Evidence: versioned load-test configuration, report, and failure-injection results.

### TS-404 — Assemble first-release verification evidence

**Priority:** P0 / release gate<br>
**Maps to:** all P0 stories and epic definition of done<br>
**Blocked by:** TS-301–TS-403

Run the product epic's verification matrix and collect traceable evidence for the release decision.

Acceptance criteria:

1. The held-out benchmark has at least 50 independently reviewed feasible cases, including at least 20 shape and 20 text cases, plus at least 20 restriction/difficult/no-match cases.
2. At least 80% of feasible cases yield a qualifying full trace recognised by two of three blind reviewers, reported separately for shapes and text.
3. Every returned benchmark route passes continuity, fixed-start, access/turn, distance, version consistency, and GPX round-trip checks; one invalid route blocks release.
4. The anonymous manual-start-to-real-route-to-GPX journey passes on current Chrome, Firefox, Safari, mobile Chrome, and mobile Safari available at release.
5. Provider attribution, usage, retention, caching, export, and cost obligations are reviewed against the deployed configuration.
6. Unresolved coverage, browser, provider, algorithm, and quality limitations are documented next to the release decision.

Evidence: signed release checklist linking benchmark, browser, importer, accessibility, security, performance, and provider-review artifacts.

## Stage 5 — production implementation

### TS-501 — Execute queued route jobs in the production worker

**Priority:** P0<br>
**Maps to:** production route generation and US05–US10<br>
**Blocked by:** TS-107, TS-202, TS-205, TS-401, TS-402

Replace the production worker's no-op work operation with the durable, fenced route-generation pipeline. This story owns the shared worker-to-persistence contract and must publish a handoff before dependent stories consume it.

Acceptance criteria:

1. The production worker claims queued PostgreSQL jobs, loads the exact immutable request revision and versioned template/profile/map inputs, and rejects missing or inconsistent inputs without generating a route.
2. Heart and curated-text jobs invoke the real pinned routing implementation and persist only fully validated candidate snapshots; no fixture, straight-line, car-route, inferred access, or fabricated geometry can enter the production path.
3. One successful attempt publishes one to three distinct candidate IDs and a terminal match outcome using lease-fenced writes; no-match and classified dependency/provider failures publish truthful terminal outcomes.
4. Cancellation, deadline expiry, lease renewal/loss, bounded retry, provider-call budget exhaustion, process termination, and database reconnect stop or resume work without duplicate candidate publication or false success.
5. Progress stages and privacy-safe metrics come from real execution and never expose exact coordinates, full traces, credentials, cookies, tokens, or custom text.
6. Tests cover success, no-match, cancellation during routing, timeout, stale lease, duplicate delivery, worker restart, persistence failure, malformed stored input, and unknown version behavior against PostgreSQL.

Evidence: focused worker/persistence integration suite, production-path dependency test proving `take_work` is not a no-op, and QA-reviewed worker contract handoff.

### TS-502 — Prove the connected route-generation vertical slice

**Priority:** P0<br>
**Maps to:** P0 anonymous planner journey<br>
**Blocked by:** TS-204, TS-206, TS-207, TS-305, TS-501

Exercise the real browser/API/PostGIS/worker/routing path without injecting candidate fixtures.

Acceptance criteria:

1. A fresh anonymous session can load the catalogue, resolve and explicitly confirm a reviewed start, submit a job, observe real progress, receive a persisted candidate, inspect its exact route details, and export that same traversal as GPX.
2. The test proves that the API response, stored snapshot, rendered geometry, route details, and GPX have the same job, request revision, candidate identity, coordinate order, distances, retracing, and immutable version pins.
3. A second guest cannot read, cancel, export, or infer the first guest's job or candidate, including through guessed and malformed identifiers.
4. No-match, cancellation, timeout, dependency loss, refresh during polling, stale responses, and unavailable base-map cases remain truthful and recoverable.
5. The connected test runs in CI with PostgreSQL/PostGIS and a deterministic pinned routing graph; required database cases must not be silently skipped.

Evidence: CI end-to-end transcript, retained deterministic test inputs, exported GPX validation, and independent QA journey review.

### TS-503 — Approve reviewed production routing data and configuration

**Priority:** P0<br>
**Maps to:** coverage, start-resolution, access-policy, and immutable-data requirements<br>
**Blocked by:** TS-103, TS-107, TS-401

Replace illustrative deployment values with accountable, reproducible production-pilot data and configuration.

Acceptance criteria:

1. The production routing graph, source provenance, extraction procedure, coverage polygon or bounds, checksum, map-data version, and review date are recorded and reproducible from approved source material.
2. Every configured routable start is derived from the same pinned graph, is reviewed per supported activity profile, and is not presented as valid for an unreviewed profile.
3. Coverage and start-resolution tests include boundary, outside-coverage, isolated, direction-restricted, and unknown-data cases and never silently move a start or widen tolerance.
4. Startup fails closed for missing, malformed, mismatched, empty, or unknown-version graph/start/configuration inputs.
5. Production workload and retention values are reviewed, remain within hard caps, use canonical metres, and are recorded without embedding secrets or personal precise locations.
6. A controlled update procedure assigns new immutable versions and proves old snapshots remain interpretable.

Evidence: signed data/configuration manifest, checksums and derivation commands, boundary test report, and QA review.

### TS-504 — Harden the public first-release product surface

**Priority:** P0<br>
**Maps to:** honest production scope and TS-305 boundaries<br>
**Blocked by:** TS-305

Ensure the shipped interface exposes only implemented first-release capabilities and states its intentional limitations clearly.

Acceptance criteria:

1. Google sign-in and place-name search controls are absent or explicitly unavailable in production unless their complete authenticated/geocoding integrations, privacy review, and failure behavior are delivered in separate approved stories.
2. Guest planning remains usable without an account, and saved-route UI consistently states that routes are stored only on the current device and can be removed by clearing site data.
3. The planner and catalogue identify the supported pilot geography and calibrated designs without claiming citywide, arbitrary-start, or arbitrary-text feasibility.
4. Unsupported actions cannot be reached through direct URLs, stale client state, query parameters, keyboard navigation, or development flags in the production bundle.
5. Production, accessibility, and visual-shell tests cover the resulting navigation, unavailable states, and scope copy at mobile and desktop widths.

Evidence: production bundle inspection, navigation/accessibility tests, screenshots, and signed first-release scope record.

### TS-505 — Package and deploy the complete production stack

**Priority:** P0<br>
**Maps to:** production deployment and operational reproducibility<br>
**Blocked by:** TS-305, TS-501, TS-503

Create a reproducible staging and production deployment for the static client, API, functional worker, and PostgreSQL/PostGIS.

Acceptance criteria:

1. Versioned, immutable runtime artifacts build with locked dependencies for the target platform; API and worker run as non-root processes, handle termination, and expose or support appropriate liveness/readiness checks.
2. Infrastructure configuration provisions separate staging and production environments, PostgreSQL 16/PostGIS 3.4 or an explicitly reviewed compatible version, persistent encrypted storage, API and worker compute, and immutable client hosting.
3. One canonical HTTPS origin serves the client and proxies `/api/*`; cookie, Origin/CSRF, security-header, request-size, timeout, and privacy-safe proxy logging behavior is verified through the edge.
4. Secrets come from a managed secret store, migrations run as a separately authorized forward-only release job, and neither images nor client bundles contain production credentials or illustrative configuration.
5. The delivery pipeline records source revision and artifact digests, deploys to staging first, performs readiness and connected smoke checks, supports controlled binary rollback, and never deploys the known no-op worker.
6. A clean-environment deployment runbook names owners, prerequisites, promotion steps, health checks, rollback conditions, and teardown protections.

Evidence: infrastructure definitions, artifact/SBOM or dependency manifest, staging deployment record, edge tests, and rollback rehearsal.

### TS-506 — Meet production route-generation performance gates

**Priority:** P0<br>
**Maps to:** TS-403 nonfunctional release gates<br>
**Blocked by:** TS-403, TS-502, TS-505

Remediate the measured 68.886-second p95 and 75.507-second maximum using the connected worker on production-equivalent infrastructure.

Acceptance criteria:

1. Profiling separates queue, graph loading, candidate search, provider, validation, snapshot persistence, and total latency without logging precise route data.
2. Algorithm, concurrency, caching, admission-control, or capacity changes preserve every legality and truthfulness invariant and have focused regression tests.
3. A repeatable production-equivalent run covers at least 20 concurrent representative jobs and reports p50/p95/p99 acknowledgement, queue, provider/search, persistence, and end-to-end latency with terminal outcomes and resource saturation.
4. Accepted-job acknowledgement p95 is at most one second, termination p95 is at most 30 seconds, and every job terminates within 60 seconds.
5. If an accountable release owner proposes different thresholds, the change is a separately signed product/SRE decision made before measurement; this story does not pass by retrospectively weakening a missed gate.
6. Load shedding and overload states return bounded, truthful retry or unavailable responses and do not starve cancellation, cleanup, or lease renewal.

Evidence: versioned workload, profiler record, before/after report on recorded hardware, invariant regression suite, and independent QA/SRE sign-off.

### TS-507 — Operationalize privacy-safe monitoring and incident response

**Priority:** P0<br>
**Maps to:** production operations, privacy, and US14<br>
**Blocked by:** TS-402, TS-505

Connect the checked-in telemetry contracts to the deployed monitoring and response systems.

Acceptance criteria:

1. Production dashboards show API health, acknowledgement, queue delay/backlog, stuck leases, worker throughput, route-search latency, outcomes, provider failures, timeouts, cancellations, and cleanup without high-cardinality or precise-route labels.
2. Alerts are imported, tested with controlled signals, routed to named owners, and linked to runbooks containing triage, escalation, mitigation, and closure steps.
3. Edge, application, database audit, monitoring, error-reporting, and support paths are reviewed to exclude query strings, bodies, cookies, authorization/CSRF headers, exact coordinates, full traces, custom text, credentials, and tokens.
4. Correlation IDs connect browser request, API request, job, attempt, worker, provider call, and terminal outcome without identifying a person or location.
5. Monitoring-system outage, alert-delivery failure, cleanup failure, and stale-worker scenarios have observable degraded states and tested operator procedures.

Evidence: deployed dashboard/alert links or exports, alert-fire transcript, end-to-end redaction audit, on-call ownership record, and QA review.

### TS-508 — Prove backup restore recovery and rollback

**Priority:** P0<br>
**Maps to:** durability, retention, and operational recovery<br>
**Blocked by:** TS-401, TS-501, TS-505

Turn the documented recovery expectations into tested production procedures.

Acceptance criteria:

1. Automated encrypted backups use access-audited storage, have a retention period no longer than seven days, and expose success, age, and failure monitoring without leaking route data.
2. A restore into an isolated environment verifies migration checksums, snapshot readability, authorization isolation, and expiry cleanup before any traffic is admitted.
3. Database interruption, API restart, worker termination during fitting, stale lease, partial candidate persistence, and network partition recover without duplicate publication, corrupted snapshots, or false success.
4. Application rollback is rehearsed against forward-only migrations with an explicit compatibility window and a stop condition when rollback is unsafe.
5. Accountable owners approve measured recovery-point and recovery-time results, data-deletion behavior after restore, and the disaster-recovery runbook.

Evidence: backup and restore logs with sensitive values removed, failure-injection report, rollback transcript, measured RPO/RTO, and independent QA/operations review.

### TS-509 — Close deployed provider licensing capacity and cost obligations

**Priority:** P0<br>
**Maps to:** TS-404 provider release gate<br>
**Blocked by:** TS-503, TS-505

Review providers and derived data against the actual deployed origin, configuration, and expected traffic.

Acceptance criteria:

1. The deployed tile/routing configuration records providers, endpoints, attribution, credentials classification, origin/referrer behavior, contact channel, user agent where applicable, and cache behavior observed through the production edge.
2. Expected and tested request volume fits each provider's policy and capacity; a paid, hosted, or self-hosted alternative replaces a best-effort public service when accountable review finds it unsuitable.
3. Outage, throttling, quota, and slow-response behavior preserves forms and generated route data and displays explicit unavailable states without substituting another routing mode.
4. ODbL and other applicable obligations are reviewed for the pinned source, derived routing database, route exports, attribution, notices, and any publication or distribution plan.
5. Monthly compute, storage, backup, bandwidth, tile/provider, monitoring, and support costs are estimated at pilot and agreed growth levels, with owners and budget approval recorded.

Evidence: deployed provider review, policy/caching capture, load/cost model, licensing decision, failure test, and accountable approval.

## Stage 6 — independent release validation

### TS-601 — Pass the independently reviewed held-out route benchmark

**Priority:** P0 / release gate<br>
**Maps to:** TS-404 held-out quality and validity gates<br>
**Blocked by:** TS-301, TS-502, TS-503, TS-506

Run the required benchmark against frozen production-candidate artifacts without tuning on the held-out answers.

Acceptance criteria:

1. The frozen set contains at least 50 independently reviewed feasible cases, at least 20 shapes, at least 20 text cases, and at least 20 restriction/difficult/no-match cases, with provenance and selection rules that prevent test leakage.
2. Three independent blind reviewers assess each feasible output, two recognitions are required to qualify, and shape and text results are reported separately with inter-reviewer disagreement retained.
3. At least 80% of feasible shape cases and at least 80% of feasible text cases produce a qualifying full trace; no missing result is reclassified or omitted after execution.
4. Every returned route passes continuity, exact confirmed start, access/direction/turn policy, requested distance tolerance, connector/retrace disclosure, immutable version consistency, authorization, and GPX round-trip checks; one invalid route fails the gate.
5. Raw case identities, outputs, reviewer decisions, scripts, environment/artifact versions, exclusions, and aggregate calculations are retained in a privacy-safe, reproducible evidence package.

Evidence: complete 50-case benchmark package, blind-review records, automated validity output, aggregate report, and independent QA signature.

### TS-602 — Pass the five-browser real-route release journey

**Priority:** P0 / release gate<br>
**Maps to:** TS-404 browser journey gate<br>
**Blocked by:** TS-502, TS-504, TS-505, TS-509

Manually verify the deployed release candidate on every required browser using real generated routes.

Acceptance criteria:

1. Current Chrome and Firefox on desktop, current Safari on macOS, Mobile Chrome on a physical supported Android device, and Mobile Safari on a physical supported iOS device each complete a signed anonymous start-to-real-route-to-GPX journey.
2. Each run covers catalogue loading, coordinate or map start entry, explicit snap confirmation when proposed, job progress, route comparison/detail, attribution, local save/reopen, exact GPX export, and import into an independent GPX consumer.
3. Each run also checks cancellation, a truthful unavailable/error state, refresh recovery, keyboard or relevant assistive navigation, no page-level overflow, and preserved form/result data when the base map is unavailable.
4. Evidence records browser/OS/device versions, deployed origin, release revision and artifact digests, job/candidate correlation identifiers that contain no personal data, tester, timestamp, result, and linked defect disposition.
5. Emulation, viewport screenshots, automation, or an unavailable test environment cannot be recorded as a manual pass.

Evidence: signed five-target matrix, redacted journey captures, independent GPX import records, and resolved-defect links.

### TS-603 — Issue the independently signed production release decision

**Priority:** P0 / release gate<br>
**Maps to:** epic definition of done and production authorization<br>
**Blocked by:** TS-404, TS-506–TS-509, TS-601, TS-602

Replace the historical NO-GO with a traceable decision for one immutable release candidate.

Acceptance criteria:

1. The checklist identifies the exact source revision, client/API/worker artifact digests, database migration set, routing-data/configuration versions, deployed origin, and decision timestamp.
2. Held-out quality, route validity, browser journey, provider obligations, performance, accessibility, security/privacy, GPX import, deployment, monitoring, backup/restore, and rollback gates each link to reviewed evidence and are fail-closed.
3. `npm run factory:verify`, the connected end-to-end suite, deployment smoke tests, and all non-automatable checklists pass against the release candidate with no required database cases skipped.
4. Known limitations are accurate, visible, and accepted by the accountable product/operations owner; no blocker is relabelled as a limitation merely to authorize release.
5. Developer, independent QA, product/release owner, and operations owner sign the same decision; any non-passed gate results in NO-GO.
6. The promotion plan names the release operator, monitoring window, rollback authority, stop conditions, and post-deployment verification, and the resulting production state is recorded after execution.

Evidence: immutable signed checklist, complete evidence map, successful CI/deployment references, production smoke record, and post-release verification.

## Deferred work

The following stay out of the first implementation sequence even though the prototype includes some design states:

- Google authentication, cloud route storage, and sharing begin with P1 US17. Guest planning and on-device saving do not depend on them.
- Arbitrary short text begins with P1 US15 after the curated text benchmark passes.
- Foreground route following begins with P1 US16 and is not implied by route preview or GPX export.
- Recorded-activity comparison begins with P1 US18.
- Freehand/image conversion, social/community features, competitions, subscriptions, and reliable background recording remain outside the defined scope.

## Cross-story definition of done

A story is complete only when:

1. Its acceptance criteria have automated evidence where automation is practical and linked manual evidence otherwise.
2. Public API changes update the OpenAPI contract, examples, Rust contract tests, and client decoders in the same change.
3. Schema changes include a migration and upgrade test; published template/profile/algorithm behavior receives a new version rather than mutation in place.
4. Logs and analytics are reviewed for exact coordinates, full traces, credentials, tokens, and personal data.
5. User-visible failure, cancellation, timeout, unknown-data, and stale-result behavior is implemented alongside the success path.
6. Formatting, linting, unit, integration, contract, and relevant browser tests pass in CI.
7. The implementation never invents network connectivity, omits travelled connectors, silently moves the start, widens tolerance, or substitutes illustrative geometry.
