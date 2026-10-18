# TS-502 connected vertical-slice handoff

- Version: `connected-route-slice-v1`
- Producer: TS-502
- Consumers: TS-506, TS-601, TS-602
- Compatibility: additive test and evidence contract; public HTTP and snapshot schemas are unchanged

## Contract

`npm run test:connected` is the mandatory PostgreSQL/PostGIS-backed release-path test. It fails if
`DATABASE_URL` is absent or cannot create an isolated database; it never converts the required case
to a skip. The CI verify job runs it explicitly against the PostGIS service before the workspace
suite.

The test uses the approved production routing manifest and checksum-pinned graph. Through the real
API router it creates a private guest, reads the catalogue, resolves and confirms a reviewed start,
submits an immutable request, observes queued and persisted execution progress, runs the production
worker, reads the authorized candidate, and exports it. It does not inject a candidate or routing
geometry.

The persisted snapshot, authorized API response, browser-consumed ordered geometry, and externally
parsed GPX must retain the same job ID, revision, candidate ID, coordinate order, distance fields,
retracing disclosure, and immutable version pins. A second guest receives only the normal not-found
boundary for existing and guessed IDs and cannot cancel the first guest's job; malformed IDs remain
the documented generic bad-request response.

The approved manifest now pins the canonical LF extraction-script bytes. `INC-054` records why the
previous CRLF-derived checksum made a fresh checkout fail closed. The immutable routing graph and
map-data version did not change.

## Verification

```text
npm run test:connected
npm exec -- tsx --test tests/planner-workflow.test.ts tests/route-candidate-journey.test.ts
node scripts/cargo.mjs test -p gpsart-worker --all-targets -- --test-threads=1
npm run factory:verify
```
