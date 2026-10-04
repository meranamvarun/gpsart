# TS-105 local-graph prerequisite evidence

## Source and immutability

- Source: OpenStreetMap API 0.6 map response for the bounded URL recorded in `cubbon-park-osm-v1.json`, retrieved 2026-10-04T03:11:34Z.
- Raw response SHA-256: `3828e4099d571653e7cd1ee05caa73dadb5ec34ad99b80906cc24a0be3a5c5da`.
- Derived snapshot SHA-256: `c4a009bef6385175de8762d58ff604c42e7419c6795a2774466c6fcf49e0db48`.
- Content: 40 versioned nodes, five versioned ways, 40 way segments, and no touching OSM relations. Contributor account fields were removed.
- Licence/provenance: OpenStreetMap contributors, ODbL 1.0, carried on each route result.

The checked-in extractor refuses any raw response other than the recorded bytes and refuses to omit a relation touching a selected way. The loader independently verifies the derived checksum before parsing and rejects missing versions, duplicate identities/points, invalid coordinates, absent references, zero-length segments, invalid turn references, unknown schema fields, or unsupported schema versions.

## Truth-preserving routing checks

`crates/gpsart-routing/tests/local_osm_snapshot.rs` verifies:

1. Checksum mismatch is reported before malformed JSON is parsed.
2. Missing element versions invalidate the snapshot.
3. Missing direction remains unknown and cannot be guessed as bidirectional.
4. Explicit foot-only access accepts walking and rejects cycling; missing access stays visibly qualified as unknown.
5. Coordinate anchors must exactly equal a retained graph node, including at E7 precision.
6. Dateline distance calculation neither overflows integer arithmetic nor chooses the long arc.
7. A real Bengaluru traversal succeeds for walking and cycling with 28 ordered segment occurrences, exact first/last nodes, per-edge distance reconciliation, immutable checksum provenance, and attribution.
8. A four-anchor Bengaluru traversal repeats a real segment twice and counts both occurrences in the total.

Existing controlled-graph tests independently cover forbidden turns, disconnected/grade-separated topology, profile exclusions, cancellation, deadlines, and the shared atomic attempt budget.

## Commands and observed result

```powershell
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
node scripts/cargo.mjs test -p gpsart-routing --test local_osm_snapshot
node scripts/cargo.mjs test -p gpsart-routing -- --test-threads=1
node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings
npm run factory:verify
```

Before QA, the focused suite passed 7/7 and the full routing crate passed 23/23. The focused suite reported 0.01 seconds of test execution after compilation. The complete `factory:verify` gate passed, including Rust format/clippy/workspace tests/build, 8 factory tests, 48 TypeScript unit tests, 2 visual-shell tests, 4 Sites tests, typecheck, and production build.

On the first fresh-worktree factory run, the existing command order executed `test:sites` before `build`, so its packaging assertion could not find an unbuilt `dist/client/index.html`; that same run then built the artifact successfully. An unchanged rerun of the canonical factory command passed every gate. No product assertion failed.

Independent QA result: pending.
