# Provider obligations review

Reviewed 2026-10-11 against the production client configuration and immutable local routing snapshot contract.

## OpenStreetMap data and routing

- Routing uses a pinned, local OSM-derived artifact; it does not silently fall back to a remote routing service.
- OSM data attribution is retained in route snapshots and is now emitted in GPX 1.1 metadata.
- Immutable provider, map, profile, algorithm, and template versions remain attached to results.
- OSM data is licensed under ODbL. Any distribution of a derived database must be assessed for ODbL share-alike obligations before that database is published.
- There are no routing-provider credentials or per-call fees in the recorded configuration. Hosting, compute, storage, and bandwidth costs are deployment costs and are not yet measured.

## OpenStreetMap raster tiles

- The client uses the required `https://tile.openstreetmap.org/{z}/{x}/{y}.png` endpoint and presents a visible link to the OpenStreetMap copyright page.
- Tile requests are interactive only. The application does not implement bulk download, prefetch, offline tile archives, or a custom cache that overrides normal browser caching.
- The public tile service is best effort and has no SLA. A slow or unavailable base map must not remove form state or generated route data; that behavior is covered by TS-403.
- No tile credential is used. Before public deployment, operators must ensure the production origin has an identifiable referrer/user agent, provide a contact channel, monitor usage, and be ready to move to another compliant tile service if asked.

Sources reviewed:

- OpenStreetMap Foundation, [Tile Usage Policy](https://operations.osmfoundation.org/policies/tiles/)
- OpenStreetMap Foundation, [Copyright and License](https://www.openstreetmap.org/copyright/en)

Review result: source configuration passes the reviewed attribution, endpoint, caching, export, and credential requirements. Public-service capacity and deployment cost remain release limitations, not measured assurances.
