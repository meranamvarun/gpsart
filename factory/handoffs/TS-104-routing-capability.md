# TS-104 routing capability handoff

**Version:** 1  
**Producer:** `codex/dev-ts104`  
**Consumers:** TS-105, TS-106, TS-107 and routing-provider deployment work  
**Compatibility:** additive routing boundary; changing route ordering, control errors, or provenance requires coordinated consumer updates

## Published contracts

- `RouteProvider::route` accepts a versioned profile, ordered anchors, and `AttemptControl`.
- `RoutePath` returns ordered edge occurrences, canonical distance, ordered geometry, manoeuvres, and provider/data/attribution provenance. Repeated edge IDs remain repeated occurrences.
- `ControlledGraph` is deterministic test infrastructure. It enforces direction, forbidden turns, activity access, component separation, and graph topology at grade-separated crossings.
- `AttemptControl` propagates cancellation and a per-call deadline and atomically limits attempts. `route_with_retries` cannot exceed that shared budget.
- `ValhallaAdapter` accepts coordinate anchors, maps walking/running to pedestrian and cycling to bicycle, requests exact `edge_walk` attribution, decodes polyline6 geometry, and rejects absent/malformed edges. It marks access as unverified.

## Decision and blocker

ADR `docs/adr-001-routing-provider-spike.md` records **replace/evaluate local graph**. The public Valhalla setup was not adopted because it did not provide exact edge attribution for the returned multi-anchor routes on an immutable snapshot. TS-105 must not consume diagnostic `map_snap` output and remains blocked pending a pinned local graph capability proof.

## Verification

```powershell
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
npm run factory:verify
```

Focused suites are `cargo test -p gpsart-routing --test controlled_graph`, `cargo test -p gpsart-routing --test valhalla_adapter`, and the inherited `profile_policy` suite.
