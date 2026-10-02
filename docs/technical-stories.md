# GPSART technical delivery backlog

Status: initial implementation backlog<br>
Prepared: 2 October 2026<br>
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
