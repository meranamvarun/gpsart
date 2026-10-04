# TS-107 candidate quality and benchmark verification

## Acceptance trace

1. `CandidateQualityMetrics` records versioned symmetric full-trace error, target-distance error, repeated and connector ratios, bend complexity, identifying-feature checks, and the exact weights. The resemblance input is the canonical full routed geometry. Feature outcomes remain explicitly `unknown` until calibrated feature-specific checks exist, preventing fabricated heart/text success claims.
2. `score_candidate` rechecks immutable template identity, inclusive distance bounds, closure, continuous occurrence coverage, exact occurrence-distance sum, and connector/repeated diagnostics before calculating quality. Network legality is enforced by `CandidateSearch` before construction, but the current public candidate record cannot independently prove that history at the scoring boundary. QA correctly rejected the earlier stronger claim.
3. Ranking is deterministic, including equal-density representative selection and a zero limit. Traversal identity includes direction. Ordered edge-traversal duplicates and forward-collinear point-density variants collapse; direction reversals, collinear backtracking, and genuinely different traversals remain eligible.
4. The numeric penalty stays private. Serialized user quality contains only `strong_match` or `approximate_match`; tests assert no score or percentage field is exposed.
5. `BenchmarkHarness` requires map, profile, template, and algorithm pins and records latency, provider calls, outcome, validity, quality metrics, and normalized route-artifact paths. It rejects missing pins and contradictory success fields.
6. The checked-in harness runs one shape and one curated-text search on the immutable public Cubbon Park OSM snapshot. Both are structurally valid `approximate_match` outputs, but independent visual review found the SVGs are the same repeated oval and do not demonstrate a recognisable reviewed heart and HI traversal. Consequently, acceptance criterion 6 and the Stage-1 feasibility exit remain blocked.

## Benchmark evidence

- Machine-readable results: `ts-107-benchmark-results.json`.
- Reviewed normalized full traces: `ts-107-pilot-heart.svg` and `ts-107-pilot-hi.svg`.
- Map pin: `cubbon-park-osm-v1` plus derived SHA-256 `c4a009bef6385175de8762d58ff604c42e7419c6795a2774466c6fcf49e0db48`.
- Profile pin: `walking-v1`.
- Template pins: immutable published `heart@1` and `HI@1` hashes.
- Algorithm pin: `candidate-quality-v1`; weight pin: `candidate-quality-weights-v1`.
- Both observed cases made one provider call and retained the canonical full route. Their very high repeated ratios and unknown feature outcomes are preserved in the report rather than hidden.
- The JSON schema contains no coordinate or geometry fields. SVG coordinates are normalized drawing coordinates and explicitly omit geographic coordinates.

## Verification

Independent reviewer `codex/qa-ts107` rejected commit `fa3c839` on 2026-10-04. The review found reversal/direction dedup defects, fabricated shared feature checks, nondeterministic representative selection, a zero-limit bug, an unproven legality-boundary claim, and inadequate pilot artifacts. INC-023 through INC-026 plus focused regressions address the code-level defects. The pilot feasibility and legality-evidence issues remain blockers, so the story is not approved or complete.
