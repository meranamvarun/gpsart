# TS-503 production routing data and configuration verification

Date: 2026-10-17
Developer: `codex/ts503-dev`

## Executable acceptance checks

1. Load the approved manifest and verify the graph checksum, checksum-derived map-data version, embedded OSM source provenance, extractor checksum/command, graph-wide coverage bounds, approval date, and reviewer sign-off.
2. Verify each start is an exact node in that graph and is eligible only for its listed profile versions; prove cycling is unavailable rather than inherited from walking/running review.
3. Exercise inclusive boundary and outside-coverage behavior, exact-point routing, and rejection of isolated, direction-blocked, and unknown-access nodes without moving a point or widening tolerance.
4. Reject absent or malformed manifests, unknown manifest/profile versions, graph/checksum/provenance/version mismatches, empty starts, coordinate mismatches, invalid coverage, unapproved state, and extractor mismatch.
5. Require runtime limits to equal the reviewed manifest values, with distance named and represented in canonical metres and retention within TS-401's hard cap. The manifest contains no credentials, tokens, cookies, or personal precise locations.
6. Load historical v1, v2, and v3 graph artifacts by their individual checksums and require three distinct immutable data versions. Follow the append-only update/rollback runbook for successors.

Approved manifest SHA-256: `7b738c0b6659569082d5aebd30ce1d7293cd8c78c3b1d1ad54378d62c810672c`. Git forces the manifest, graph, checksum, and extractor inputs used for hashing to LF so verification is platform-stable.

## TDD record

The new `production_routing` test was added before its implementation. Its initial compile failed because `ProductionRoutingConfig` and `ProductionRoutingError` did not yet exist. The implementation then added the smallest shared loader and graph inspection boundary needed to satisfy the checks, followed by API startup integration.

## Results

- `cargo test -p gpsart-config --test production_routing`: 4 passed.
- `cargo test -p gpsart-routing --test local_osm_snapshot`: 10 passed.
- `npm run test:factory`: 21 passed.
- `cargo check -p gpsart-api`: passed.
- `npm run factory:verify`: not completed. The first recovered run failed because the isolated worktree lacked its ignored local toolchain and dependencies. After linking those inputs and selecting the repository's GNU toolchain, two bounded runs stalled inside Cargo/Clippy after compiler activity (20 minutes with the isolated target; 10 minutes with the shared verified target). No full-gate pass is claimed.

The checked-in raw OSM API response is intentionally omitted because it can contain contributor-identifying fields. The approved source URL, retrieval time, raw SHA-256, derived sanitized graph, graph SHA-256, extractor command, and extractor SHA-256 provide the reproducibility record without embedding that personal data in fixtures.
