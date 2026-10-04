# TS-104 routing capability handoff

**Version:** 2
**Producer:** `codex/dev-ts104`, extended by `blocker/ts105-local-graph`
**Consumers:** TS-105, TS-106, TS-107 and routing-provider deployment work  
**Compatibility:** additive routing boundary; changing route ordering, control errors, or provenance requires coordinated consumer updates

## Published contracts

- `RouteProvider::route` accepts a versioned profile, ordered anchors, and `AttemptControl`.
- `RoutePath` returns ordered edge occurrences, canonical distance, ordered geometry, manoeuvres, and provider/data/attribution provenance. Repeated edge IDs remain repeated occurrences.
- `ControlledGraph` is deterministic test infrastructure. It enforces direction, forbidden turns, activity access, component separation, and graph topology at grade-separated crossings.
- `AttemptControl` propagates cancellation and a per-call deadline and atomically limits attempts. `route_with_retries` cannot exceed that shared budget.
- `ValhallaAdapter` accepts coordinate anchors, maps walking/running to pedestrian and cycling to bicycle, requests exact `edge_walk` attribution, decodes polyline6 geometry, and rejects absent/malformed edges. Edge intervals must cover the geometry contiguously and in order, trace distances must reconcile with the summary within documented provider-rounding tolerance, and manoeuvres must be present, ordered, and bounded. It marks access as unverified.

## Decision and resolved blocker

ADR `docs/adr-001-routing-provider-spike.md` records **replace/evaluate local graph**. The public Valhalla setup remains rejected because it did not provide exact edge attribution for returned multi-anchor routes on an immutable snapshot. Diagnostic `map_snap` output remains prohibited.

ADR `docs/adr-002-adopt-local-osm-pilot-graph.md` now adopts `LocalOsmGraph` for Stage 1 feasibility. It loads a checksum-pinned, versioned OSM-derived artifact, requires exact retained-node anchors, returns ordered segment occurrences and checksum provenance, and exposes unknown access/direction rather than guessing. Real walking and cycling traversals plus repeated occurrences are covered on the pinned Bengaluru pilot bytes. This resolves the local-graph prerequisite and unblocks TS-105 within the documented pilot boundary; it is not city-wide production coverage.

## Verification

```powershell
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
npm run factory:verify
```

Focused suites are `cargo test -p gpsart-routing --test controlled_graph`, `cargo test -p gpsart-routing --test local_osm_snapshot`, `cargo test -p gpsart-routing --test valhalla_adapter`, and the inherited `profile_policy` suite.
