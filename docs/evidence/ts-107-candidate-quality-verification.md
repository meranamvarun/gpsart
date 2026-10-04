# TS-107 candidate quality and benchmark verification

## Acceptance trace

1. `CandidateQualityMetrics` records versioned symmetric full-trace error, target-distance error, repeated and connector ratios, bend complexity, identifying-feature checks, and the exact weights. The resemblance input is the canonical full routed geometry, including access, connectors, retracing, and return travel.
2. `score_candidate` rechecks immutable template identity, inclusive distance bounds, closure, continuous occurrence coverage, exact occurrence-distance sum, and connector/repeated diagnostics before calculating quality. Tests prove an out-of-range route and a discontinuous route are rejected rather than offset by quality.
3. Ranking is deterministic. Ordered edge-traversal duplicates and collinear point-density-only variants collapse; a route with a genuinely different traversal remains eligible.
4. The numeric penalty stays private. Serialized user quality contains only `strong_match` or `approximate_match`; tests assert no score or percentage field is exposed.
5. `BenchmarkHarness` requires map, profile, template, and algorithm pins and records latency, provider calls, outcome, validity, quality metrics, and normalized route-artifact paths. It rejects missing pins and contradictory success fields.
6. The checked-in harness runs one shape and one curated-text case on the immutable public Cubbon Park OSM snapshot. Both are legal complete traversals and are honestly classified `approximate_match`: heavy one-way-loop retracing causes every identifying-feature check to fail. This evidence demonstrates the feasibility traversal and also preserves the pilot fixture's current quality limitation instead of presenting it as a strong match.

## Benchmark evidence

- Machine-readable results: `ts-107-benchmark-results.json`.
- Reviewed normalized full traces: `ts-107-pilot-heart.svg` and `ts-107-pilot-hi.svg`.
- Map pin: `cubbon-park-osm-v1` plus derived SHA-256 `c4a009bef6385175de8762d58ff604c42e7419c6795a2774466c6fcf49e0db48`.
- Profile pin: `walking-v1`.
- Template pins: immutable published `heart@1` and `HI@1` hashes.
- Algorithm pin: `candidate-quality-v1`; weight pin: `candidate-quality-weights-v1`.
- Both observed cases made one provider call, returned a structurally valid match, and retained the canonical full route. The exact observed latency and quality metrics are in the machine-readable report.
- The JSON schema contains no coordinate or geometry fields. SVG coordinates are normalized drawing coordinates and explicitly omit geographic coordinates.

## Verification

The implementation is verified with the focused quality and harness tests, regenerated benchmark artifacts, the complete routing suite, strict clippy, and the repository factory gate. Final command results and independent QA findings are recorded in `factory/stories.json` when the story is approved.
