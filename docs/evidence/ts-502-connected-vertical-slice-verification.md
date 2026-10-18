# TS-502 connected vertical-slice verification

Date: 2026-10-18

## Executable acceptance checks

1. `connected_vertical_slice` uses a fresh private guest and the real catalogue, approved start
   resolver, PostgreSQL stores, production worker, checksum-pinned graph, authorized route endpoint,
   and GPX endpoint. No candidate fixture or synthetic geometry enters the path.
2. The test compares the terminal job and stored `CandidateSnapshot` with the API payload, then
   passes that exact live payload through the TypeScript decoder, candidate state, map model, and
   details renderer. Those layers must agree on job/revision/candidate identity, ordered coordinates,
   distance and retracing fields, and every immutable version pin. The external GPX 1.1 import must
   preserve exact coordinate order and its documented identity/version/traversal metadata.
3. A second private guest cannot read or cancel the job and cannot read or export the candidate.
   Existing and guessed valid IDs return the same not-found boundary; malformed IDs return the same
   generic invalid-path response.
4. `production_pipeline` covers real no-match, mid-routing cancellation, timeout, provider/persistence
   dependency failure, stale lease, restart, and duplicate delivery. `planner-workflow.test.ts` covers
   refresh recovery, cancellation fencing, and stale responses. `route-candidate-journey.test.ts`
   proves a base-map failure leaves the complete routed geometry and facts available.
5. `npm run test:connected` requires `DATABASE_URL`, creates an isolated migrated database, and is an
   explicit CI step against PostGIS. Absence is a failure, not a skip.

## Deterministic inputs and transcript

- Routing manifest: `ops/routing/production-pilot-v1.json`
- Graph: `crates/gpsart-routing/data/bengaluru-central-osm-v3.json`
- Reviewed walking start: the first manifest start
- Template/distance: `heart` v1, 1,300 m with the reviewed 20% tolerance
- Local database-backed run plus live browser decode/render: 1 connected Rust test passed, 0 failed,
  0 ignored in 2.92 seconds; the browser verifier accepted and rendered that exact ephemeral API
  payload (2026-10-18).
- Production-manifest regression: 1 passed, 0 failed after `INC-054` correction.
- The first clean-worktree factory run exposed `INC-055`: Sites tests preceded the build and failed
  on the absent `dist/client/index.html` after all preceding gates passed. The ordering regression
  now requires a production build before Sites packaging tests.
- Independent QA exposed a race in the cancellation check (`INC-056`) and coordinate-bearing
  assertion diagnostics (`INC-057`). Cancellation is now held deterministically after the durable
  claim, and route-coordinate comparisons fail with fixed privacy-safe messages. The cancellation
  case passed five consecutive database-backed runs.
- Final database-backed `npm run factory:verify`: passed in 71.6 seconds after both QA fixes; the
  connected Rust test executed with 0 ignored cases.

Independent QA must rerun the commands in the handoff and review this evidence before setting the
story to done.
