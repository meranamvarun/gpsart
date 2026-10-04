# TS-107 candidate-quality handoff

**Version:** 3
**Producer:** `codex/root`
**Consumers:** TS-205, TS-206, TS-301, TS-403
**Compatibility:** replaces version 2; changing legality evidence, advisory version, metric semantics, hard gates, quality-band rules, deduplication identity, or benchmark schema requires another coordinated versioned handoff

`CandidateSearch` is the only production constructor of `ValidatedCandidate`. `score_candidate` accepts that sealed type rather than the freely constructible `HeartCandidate`. A validated candidate retains `candidate-legality-v1` evidence containing the routing-policy version, immutable routing-data version, activity-profile version, `route-use-advisory-v1`, and positive access, direction, and turn checks. Search rejects missing or false provider evidence and every edge with `AccessUnknown` or `ProviderAccessUnverified`; callers cannot convert a raw candidate back into a scoreable value.

`osm-pedestrian-policy-v2` is the pinned local-OSM interpretation. `highway=footway`, `pedestrian`, and `steps` provide the documented implicit pedestrian access unless an explicit or conditional restriction makes access unknown or forbidden. A general road `oneway` is treated as a vehicle direction for the foot profile; pedestrian restrictions require `oneway:foot`. General one-way tags on path-like ways remain unknown without foot-specific evidence, and conditional path directions remain hard exclusions. The policy follows the documented OSM meanings for [`highway=footway`](https://wiki.openstreetmap.org/wiki/Tag%3Ahighway%3Dfootway) and [`oneway:foot`](https://wiki.openstreetmap.org/wiki/Key%3Aoneway%3Afoot); it does not infer access for an untagged ordinary road.

The versioned residual-risk advisory is: “Routing and map data can be incomplete or change. Before and during the activity, verify that every segment is currently open and lawful for your activity; obey posted signs, closures, property restrictions, and applicable laws; and choose a safe alternative or stop when conditions differ.” TS-206 and every later route-detail/export surface must present this advisory before use. It allocates real-world verification to the user but never substitutes for application-side access, direction, and turn validation.

`CandidateQualityMetrics` records symmetric full-trace error, target-distance error, repeated and connector ratios, bend complexity, every published identifying-feature check, and exact metric/weight versions. Feature outcomes remain `unknown` because no calibrated feature-specific evaluator exists; unknown features cannot receive `strong_match`. The internal weighted penalty stays private. `UserQuality` exposes only `strong_match` or `approximate_match`.

`rank_and_deduplicate` deterministically collapses identical ordered edge traversals and forward-collinear density variants. Traversal identity includes direction; reversals and genuinely different traversals remain eligible. A zero result limit returns no candidates.

`gpsart-routing-benchmark-v2` adds a required routing-policy pin to the map, profile, template, and algorithm pins. The checked-in end-to-end cases use `bengaluru-central-osm-v3` at SHA-256 `df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45`, derived from the pinned OSM API capture. Extraction retains 1,237 versioned, known-walking-access ways and 5,291 nodes, removes ways participating in omitted turn-restriction relations, ambiguous path directions, duplicate coordinates, and sub-metre segments, and retains OSM attribution. The heart and HI cases are distinct legal `approximate_match` traversals. Their evidence renders the canonical full trace with aspect-preserving, rigid orientation normalization; artwork, connectors, access/return, and retracing are visibly classified. The HI pilot regression additionally requires four artwork strokes, at most 10% scale deviation, parallel uprights and I stem, a perpendicular crossbar touching both uprights, a separate I, and less than one-third retraced distance. No geographic coordinates are embedded in the artifacts.

Verify with:

```text
node scripts/cargo.mjs test -p gpsart-routing
node scripts/cargo.mjs run -p gpsart-routing --example ts107_benchmark
node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings
npm run factory:verify
```
