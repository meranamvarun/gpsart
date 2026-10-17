# TS-503 production routing configuration handoff

**Version:** 1  
**Producer:** `codex/ts503-dev`  
**Consumers:** TS-501, TS-505, TS-509, TS-601  
**Compatibility:** additive loader and deployment input; manifest, graph, coverage, and map-data versions are immutable. A successor must use new identifiers and retain old artifacts.

## Published contract

- `GPSART_ROUTING_MANIFEST` names the single approved production routing manifest. The API no longer accepts independently supplied coverage, map version, or start JSON, preventing cross-version assembly.
- The approved v1 manifest is `ops/routing/production-pilot-v1.json`. `ProductionRoutingConfig::load` verifies its schema, accountable sign-off, graph and extractor checksums, embedded graph provenance, coverage of every graph node, exact checksum-derived map-data version, and exact graph coordinates/profile eligibility for each reviewed start.
- The immutable map-data version is `osm-bengaluru-central-pedestrian-pilot-v3:sha256:df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45`. This is exactly `LocalOsmGraph::data_version()` and the value TS-501 must compare with immutable planning requests and snapshots.
- `ProductionRoutingConfig::graph_path()` returns the verified graph path for the production worker. Consumers must load those exact bytes and must not substitute or relabel another graph.
- Starts are reviewed only for `walking-v1` and `running-v1`. Cycling resolution returns no routable start; consumers must not infer cycling validity.
- Runtime `WorkloadLimits` must exactly match the reviewed canonical-metre and retention values in the manifest. Startup fails closed on any mismatch.
- `LocalOsmGraph::is_reviewable_start` rejects isolated nodes, direction-blocked departures, unknown access, and provider-unverified access. `has_provenance`, `all_points_within`, and `data_version` expose verification-only graph facts without changing routing behavior.
- The append-only update and rollback contract is in `docs/runbooks/production-routing-data-update.md`. Historical v1, v2, and v3 graph bytes remain checksum-addressable and interpretable.

## Verification

```text
cargo test -p gpsart-config --test production_routing
cargo test -p gpsart-routing --test local_osm_snapshot
cargo check -p gpsart-api
npm run factory:verify
```
