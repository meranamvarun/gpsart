# TS-107 candidate quality and benchmark verification

## Acceptance trace

1. `CandidateQualityMetrics` records versioned symmetric full-trace error, target-distance error, repeated and connector ratios, bend complexity, identifying-feature checks, and exact weights. The resemblance input is the canonical full routed geometry. Feature outcomes remain explicitly `unknown` until calibrated feature-specific checks exist, so neither pilot case is mislabeled `strong_match`.
2. Distance and legality are hard gates. `score_candidate` accepts only a sealed `ValidatedCandidate` emitted by `CandidateSearch`. The retained evidence pins validation, routing-policy, routing-data, profile, and advisory versions. Search rejects false/missing access, direction, or turn evidence and rejects occurrence edges with unknown or provider-unverified access.
3. The local `osm-pedestrian-policy-v2` treats documented footways as pedestrian-designated, separates ordinary vehicle `oneway` from `oneway:foot`, and conservatively excludes conditional or ambiguous path directions. Focused tests cover forward/reverse walking, cycling differences, explicit restrictions, conditional restrictions, and unknown access.
4. Ranking is deterministic, including equal-density representative selection and a zero limit. Traversal identity includes direction. Exact traversals and forward-collinear density variants collapse; reversals and distinct traversals remain eligible.
5. The internal numeric penalty remains private. Serialized user quality contains only `strong_match` or `approximate_match`; tests assert no score or percentage field is exposed.
6. `gpsart-routing-benchmark-v2` requires map, profile, routing-policy, template, and algorithm pins and records latency, provider calls, outcome, validity, quality metrics, and normalized route-artifact paths.
7. The checksum-pinned Cubbon Park pedestrian snapshot produces distinct heart and HI end-to-end traversals. The heart has a visible indentation, two lobes, and lower point after a disclosed rigid orientation normalization. The HI case remains an honest `approximate_match`: ordered artwork/connector/access sections are visible, while all semantic feature checks stay unknown. Independent QA makes the final Stage-1 review decision.
8. `route-use-advisory-v1` tells users to verify current legality, access, signs, closures, property restrictions, and changing conditions. The disclaimer is residual-risk disclosure, not a replacement for application validation.

## Benchmark evidence

- Machine-readable results: `ts-107-benchmark-results.json`.
- Reviewed normalized full traces: `ts-107-pilot-heart.svg` and `ts-107-pilot-hi.svg`.
- Map pin: `cubbon-park-osm-v2+49b985cef9e9ab9338c1a83a3a7d27cc9da2136743a5a968ffcd163bb7f58a37`.
- Raw OSM capture pin: `e2fed0d29ef5f4536bd8e2abdbdc7888c5834e78582fdfe4ea403c466967a30f`, retrieved `2026-10-04T15:48:27Z` from the source URL retained in the snapshot.
- Profile and routing-policy pins: `walking-v1` and `osm-pedestrian-policy-v2`.
- Template pins: immutable published `heart@1` and `HI@1` hashes.
- Algorithm and weights: `candidate-quality-v1` and `candidate-quality-weights-v1`.
- The benchmark JSON contains no coordinate or geometry fields. SVGs use only normalized drawing coordinates, preserve aspect ratio, and apply only the disclosed rigid inverse template rotation.

## Verification

- `node scripts/cargo.mjs test -p gpsart-routing`: 48 tests passed, including the sealed legality boundary, pedestrian-policy regressions, quality/dedup checks, and checksum-pinned pilot cases (2026-10-04).
- `node scripts/cargo.mjs run -p gpsart-routing --example ts107_benchmark`: regenerated both artifacts and the v2 benchmark report (2026-10-04).
- `node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings`: passed (2026-10-04).
- `npm run factory:verify`: passed across Rust, TypeScript, factory, build, and packaging gates (2026-10-04).
- Independent QA: pending.
