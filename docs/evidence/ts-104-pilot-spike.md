# TS-104 Valhalla capability spike

## Setup and method

The proposed engine was tested on 2026-10-02 against the Valhalla project’s documented public demo endpoint, `https://valhalla1.openstreetmap.de`. The status response reported Valhalla `3.9.0-685e3d8` and `tileset_last_modified=1790924882` (`2026-10-02T07:08:02Z`). This was not a self-hosted or immutable snapshot: Docker and a local Valhalla binary were unavailable, and the service did not return an OSM changeset from the trace calls.

Three deterministic Bengaluru-area requests were executed: an eight-anchor heart with pedestrian costing, the same heart with bicycle costing, and a ten-anchor HI attempt with pedestrian costing. The captured observations are in `ts-104-valhalla-captures.json`.

The route endpoint returned shapes and manoeuvres for all three attempts. Walking and cycling therefore reached the live provider, and the heart/HI waypoint forms were accepted. These results are not labelled legal GPSART routes.

## Traversal-control result

The normal route response did not contain ordered edge identifiers. The spike passed each returned encoded shape to `trace_attributes`:

- `shape_match=edge_walk`, which is required to preserve the exact prior traversal, returned HTTP 400 for all three complex routes.
- `shape_match=map_snap` returned edge attribution, including repeated edge/way evidence, but it is a new map-matching operation and may select different connectivity. GPSART must not silently substitute that rematched path for the original traversal.
- The trace response did not publish an OSM changeset, so the public graph could not be identified as an immutable routing-data snapshot.

The implemented `ValhallaAdapter` therefore requests `edge_walk` only and rejects missing/malformed edge attribution. Provider access is carried as `ProviderAccessUnverified`; it is never upgraded to verified access.

## Operational facts and unknowns

| Concern | Observed result |
| --- | --- |
| Limits | The API documents configurable service limits and HTTP 429. This public instance’s actual waypoint, payload, and request-rate limits were not published, so they remain unknown. |
| Latency | Status: 1552 ms. Route requests: 389–1432 ms. Successful diagnostic `map_snap` traces: 455–567 ms. These are three observations, not an SLA or benchmark. |
| Attribution | The adapter requires configured attribution. Pilot calls used OpenStreetMap-derived data; production UI/export must carry OpenStreetMap contributor attribution. |
| Caching | No cache behavior or freshness guarantee was published by the public instance; unknown. |
| Retention | Request/log retention for the public instance was not published; unknown. Do not send user routes to it. |
| Deployment cost | Not measured because there was no local deployment. Hardware, tile-build time, storage, and operations cost remain unknown. |
| Waypoint/control constraints | Eight and ten ordered locations were accepted. Exact-anchor traversal and an instance maximum were not established. |
| Data version | Valhalla build and mutable tileset timestamp were observed; an immutable Bengaluru extract/checksum was not available. |

References used for the spike are the official [Valhalla route API](https://valhalla.github.io/valhalla/api/turn-by-turn/overview/), [map-matching/trace attributes API](https://valhalla.github.io/valhalla/api/map-matching/api-reference/), and [public demo description](https://valhalla.github.io/valhalla/valhalla-intro/).

## Conclusion

The proposed deployment is not adopted. It did not demonstrate that ordered provider edges, geometry, manoeuvres, restrictions, and a pinned data version all describe the same immutable route. Per TS-104’s failure branch, TS-105 remains blocked until a locally queryable graph behind the published trait proves the same capabilities on a checksum-pinned Bengaluru extract.
