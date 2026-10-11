# Provider obligations review

Reviewed 2026-10-12 against the source client configuration and immutable local routing snapshot contract. No deployed production origin or provider-usage record was available for review.

## OpenStreetMap data and routing

- Routing uses a pinned, local OSM-derived artifact; it does not silently fall back to a remote routing service.
- OSM data attribution is retained in route snapshots and is now emitted in GPX 1.1 metadata. Publication of the derived routing database or route-data exports still requires an accountable deployment review of the applicable ODbL notice and share-alike obligations.
- Immutable provider, map, profile, algorithm, and template versions remain attached to results.
- OSM data is licensed under ODbL. Any distribution of a derived database must be assessed for ODbL share-alike obligations before that database is published.
- There are no routing-provider credentials or per-call fees in the recorded configuration. Hosting, compute, storage, and bandwidth costs are deployment costs and are not yet measured.

## OpenStreetMap raster tiles

- The client uses the required `https://tile.openstreetmap.org/{z}/{x}/{y}.png` endpoint and presents a visible link to the OpenStreetMap copyright page.
- Tile requests are interactive only. The application does not implement bulk download, prefetch, offline tile archives, or a custom cache that overrides normal browser caching.
- The public tile service is best effort and has no SLA. A slow or unavailable base map must not remove form state or generated route data; that behavior is covered by TS-403.
- No tile credential is used. The repository does not prove that an actual production origin sends an acceptable referrer, exposes a contact channel, honours caching end to end, stays within acceptable usage, or has a measured operating cost. Those checks require the deployed configuration and traffic record.

Sources reviewed:

- OpenStreetMap Foundation, [Tile Usage Policy](https://operations.osmfoundation.org/policies/tiles/)
- OpenStreetMap Foundation, [Copyright and License](https://www.openstreetmap.org/copyright/en)

Review result: **BLOCKED for release**. The source-level endpoint, visible attribution, no-prefetch behavior, and export-attribution changes pass their repository checks. Usage, caching behavior, operator contactability, derived-data publication obligations, capacity, and cost have not been reviewed against a deployed configuration, so the complete TS-404 provider gate cannot pass.
