# TS-107 candidate-quality handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-205, TS-301, TS-403  
**Compatibility:** additive Rust routing contract; changing metric semantics, hard gates, quality-band rules, deduplication identity, or benchmark schema requires a coordinated versioned handoff

`gpsart-routing` exposes `score_candidate` for candidates returned after `CandidateSearch` hard validation. Scoring rechecks immutable template identity, exact distance bounds, closure, occurrence continuity, occurrence-distance reconciliation, connector/repeated diagnostics, and complete geometry coverage. Network access and turn legality remain the responsibility of the preceding provider/search validation boundary; the current public `HeartCandidate` type does not carry enough evidence for scoring to independently re-prove them. This gap must be resolved before TS-107 approval.

`CandidateQualityMetrics` records the full-trace symmetric polyline error, target-distance error, repeated and connector ratios in basis points, full-trace bend complexity, every published identifying-feature check, and the exact metric/weight versions. Feature outcomes remain explicitly `unknown` because no calibrated feature-specific evaluator exists; unknown features cannot receive `strong_match`. The internal weighted penalty is deliberately private. `UserQuality` exposes only `strong_match` or `approximate_match`; downstream APIs must not translate the internal score into a percentage.

`rank_and_deduplicate` uses a deterministic metric/traversal/canonical/raw-geometry ordering. Traversal identity retains occurrence direction. An identical ordered edge traversal collapses regardless of geometry sampling density. Only forward-continuing collinear density points are removed; reversals and distinct ordered traversals remain eligible. A zero result limit returns no candidates.

`BenchmarkHarness` emits `gpsart-routing-benchmark-v1` records with immutable map, profile, template, and algorithm pins plus latency, provider-call count, outcome, validity, quality details, and an optional normalized route artifact. The repeatable example runs heart and HI searches on the checksum-pinned public Cubbon Park snapshot. Independent review found both outputs reduce to the same heavily retraced one-way loop and do not satisfy the required reviewed shape/text pilot evidence. The current pilot snapshot also treats absent direction metadata as unknown, leaving its richer footway network ineligible. A coordinator decision or a new truth-preserving routing-data/provider handoff is required; consumers must not treat this handoff as approved.

Verify with:

```text
node scripts/cargo.mjs test -p gpsart-routing --test candidate_quality --test benchmark_harness
node scripts/cargo.mjs run -p gpsart-routing --example ts107_benchmark
node scripts/cargo.mjs test -p gpsart-routing
node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings
npm run factory:verify
```
