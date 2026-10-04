# TS-107 candidate-quality handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-205, TS-301, TS-403  
**Compatibility:** additive Rust routing contract; changing metric semantics, hard gates, quality-band rules, deduplication identity, or benchmark schema requires a coordinated versioned handoff

`gpsart-routing` exposes `score_candidate` for already generated candidates. It rechecks immutable template identity, exact distance bounds, closure, occurrence continuity, occurrence-distance reconciliation, connector/repeated diagnostics, and complete geometry coverage before any quality calculation. Invalid or out-of-range candidates return explicit `QualityError` values and cannot be rescued by resemblance.

`CandidateQualityMetrics` records the full-trace symmetric polyline error, target-distance error, repeated and connector ratios in basis points, full-trace bend complexity, every published identifying-feature check, and the exact metric/weight versions. The internal weighted penalty is deliberately private. `UserQuality` exposes only `strong_match` or `approximate_match`; downstream APIs must not translate the internal score into a percentage.

`rank_and_deduplicate` uses a deterministic metric/traversal/geometry ordering. An identical ordered edge traversal collapses regardless of geometry sampling density. Exact collinear point-density variants also collapse, while a different ordered edge traversal remains eligible.

`BenchmarkHarness` emits `gpsart-routing-benchmark-v1` records with immutable map, profile, template, and algorithm pins plus latency, provider-call count, outcome, validity, quality details, and an optional normalized route artifact. The repeatable `ts107_benchmark` example runs the heart and HI cases on the checksum-pinned public Cubbon Park snapshot and writes coordinate-free machine-readable evidence and normalized SVGs.

Verify with:

```text
node scripts/cargo.mjs test -p gpsart-routing --test candidate_quality --test benchmark_harness
node scripts/cargo.mjs run -p gpsart-routing --example ts107_benchmark
node scripts/cargo.mjs test -p gpsart-routing
node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings
npm run factory:verify
```
