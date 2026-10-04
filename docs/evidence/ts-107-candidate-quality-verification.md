# TS-107 candidate quality and benchmark verification

## Acceptance trace

1. `CandidateQualityMetrics` records versioned symmetric full-trace error, target-distance error, repeated and connector ratios, bend complexity, identifying-feature checks, and exact weights. The resemblance input is the canonical full routed geometry. Feature outcomes remain explicitly `unknown` until calibrated feature-specific checks exist, so neither pilot case is mislabeled `strong_match`.
2. Distance and legality are hard gates. `score_candidate` accepts only a sealed `ValidatedCandidate` emitted by `CandidateSearch`. The retained evidence pins validation, routing-policy, routing-data, profile, and advisory versions. Search rejects false/missing access, direction, or turn evidence and rejects occurrence edges with unknown or provider-unverified access.
3. The local `osm-pedestrian-policy-v2` treats documented footways as pedestrian-designated, separates ordinary vehicle `oneway` from `oneway:foot`, and conservatively excludes conditional or ambiguous path directions. Focused tests cover forward/reverse walking, cycling differences, explicit restrictions, conditional restrictions, and unknown access.
4. Ranking is deterministic, including equal-density representative selection and a zero limit. Traversal identity includes direction. Exact traversals and forward-collinear density variants collapse; reversals and distinct traversals remain eligible.
5. The internal numeric penalty remains private. Serialized user quality contains only `strong_match` or `approximate_match`; tests assert no score or percentage field is exposed.
6. `gpsart-routing-benchmark-v2` requires map, profile, routing-policy, template, and algorithm pins and records latency, provider calls, outcome, validity, quality metrics, and normalized route-artifact paths.
7. The checksum-pinned central-Bengaluru pedestrian snapshot produces distinct heart and HI end-to-end traversals. The heart has a visible indentation, two lobes, and lower point after a disclosed rigid orientation normalization. The HI pilot regression requires four artwork strokes, at most 10% scale deviation, parallel uprights and I stem, a perpendicular crossbar touching both H uprights, a separate I, and less than one-third retraced distance. It remains an honest `approximate_match`, and its general-purpose semantic feature checks stay unknown. Independent QA makes the final Stage-1 review decision.
8. `route-use-advisory-v1` tells users to verify current legality, access, signs, closures, property restrictions, and changing conditions. The disclaimer is residual-risk disclosure, not a replacement for application validation.

## Benchmark evidence

- Machine-readable results: `ts-107-benchmark-results.json`.
- Reviewed normalized full traces: `ts-107-pilot-heart.svg` and `ts-107-pilot-hi.svg`.
- Map pin: `bengaluru-central-osm-v3+df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45`.
- Raw OSM capture pin: `7c46353e917488745fb2de5cf24d0aadd89eeb2b53b4911b064225e36314be93`, retrieved `2026-10-04T18:08:15Z` from the source URL retained in the snapshot.
- Profile and routing-policy pins: `walking-v1` and `osm-pedestrian-policy-v2`.
- Template pins: immutable published `heart@1` and `HI@1` hashes.
- Algorithm and weights: `candidate-quality-v1` and `candidate-quality-weights-v1`.
- The benchmark JSON contains no coordinate or geometry fields. SVGs use only normalized drawing coordinates, preserve aspect ratio, and apply only the disclosed rigid inverse template rotation.

## Verification

- `node scripts/cargo.mjs test -p gpsart-routing`: 48 tests passed, including the sealed legality boundary, pedestrian-policy regressions, quality/dedup checks, and checksum-pinned pilot cases (2026-10-04).
- `node scripts/cargo.mjs run -p gpsart-routing --example ts107_benchmark`: regenerated both artifacts and the v2 benchmark report (2026-10-04).
- `node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings`: passed (2026-10-04).
- `npm run factory:verify`: passed across Rust, TypeScript, factory, build, and packaging gates (2026-10-04).
- Independent QA approved commit `da6c3e9`: the heart and HI are distinct, recognizable pilot traversals; the HI retains four identifiable artwork strokes, H crossbar/uprights, and separate I stem; the full canonical route remains visible and classified; the checksum/counts and residual-only advisory boundary were verified. Focused pilot, heart, and local-OSM tests passed 17/17, benchmark regeneration preserved geometry/metrics, and strict routing Clippy passed (2026-10-04).
