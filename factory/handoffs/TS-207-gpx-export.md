# TS-207 GPX export handoff

**Producer:** TS-207  
**Consumers:** TS-303  
**Status:** ready  
**Compatibility:** `route-gpx-v1`; changing point order, coordinate serialization, planned-route metadata, filename rules, or media/download headers requires a coordinated versioned handoff

`GET /api/v1/routes/{id}/export.gpx` authenticates the private guest session and loads the same immutable TS-205 snapshot as the route-detail endpoint. Foreign and missing IDs are indistinguishable (`404`); unavailable or invalid stored snapshots return `503`. The response is GPX 1.1 (`application/gpx+xml; charset=utf-8`) with `Content-Disposition`, `X-Content-Type-Options: nosniff`, and `Cache-Control: private, no-store`.

The export contains one track and one track segment whose points are the complete snapshot geometry in exact order. It does not reroute, simplify, split, or deduplicate the trace, so connectors, returns, repeated edges, and self-intersection traversal order remain represented. Metadata labels it as a planned route and identifies the immutable route/candidate/profile/routing-data versions. No activity time, speed, heart-rate, or completion data is emitted.

`exportRouteSnapshotGpx`, `routeExportFilename`, and `downloadRouteSnapshotGpx` in `src/domain/gpx-export.ts` are the offline browser boundary for TS-303. The browser serializer and server response are locked to the same golden fixture. Filenames use `gpsart-{sanitized-template}-{route-id}-v{candidate-version}.gpx`; unsafe or empty template components are collapsed to an ASCII-safe value or `route`.

The external `gpx` 0.10 importer opens the checked golden pilot-style self-intersecting fixture and reproduces every coordinate in order. No compatibility with untested importers is claimed.

Verification:

```text
npm exec -- tsx --test tests/gpx-export.test.ts tests/route-candidate-journey.test.ts
node scripts/cargo.mjs test -p gpsart-api --test route_snapshots
node scripts/cargo.mjs test -p gpsart-api gpx_export::tests
npm run factory:verify
```
