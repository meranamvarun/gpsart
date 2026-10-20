# Provider obligations review

Reviewed 2026-10-20 against the source client configuration, immutable local routing snapshot contract, current OSM Foundation policy, and the TS-509 planning model. No deployed production origin, traffic record, vendor quote, or accountable approval was available for review.

## OpenStreetMap data and routing

- Routing uses a pinned, local OSM-derived artifact; it does not silently fall back to a remote routing service.
- OSM data attribution is retained in route snapshots and emitted in GPX 1.1 metadata. The checked-in licensing record treats the graph as a derivative database and each route export as a Produced Work working classification; those are not legal approvals. ODbL 4.4(c)/4.6 can require a database-or-alterations offer when a Produced Work from a derivative database is used publicly, so public route display/export—not only database distribution—stays blocked pending accountable legal review and implementation of the required offer.
- Immutable provider, map, profile, algorithm, and template versions remain attached to results.
- OSM data is licensed under ODbL. Any distribution of a derived database must be assessed for ODbL share-alike obligations before that database is published.
- There are no routing-provider credentials or per-call fees in the recorded configuration. `ops/providers/monthly-cost-model.json` makes pilot and growth arithmetic reproducible, but every unit rate is explicitly an unapproved planning allowance pending a selected Linux/Kubernetes host, vendor quotes, traffic measurement, and finance approval.

## OpenStreetMap raster tiles

- The client uses the required `https://tile.openstreetmap.org/{z}/{x}/{y}.png` endpoint and presents a visible link to the OpenStreetMap copyright page.
- Tile requests are interactive only. The application does not implement bulk download, prefetch, offline tile archives, or a custom cache that overrides normal browser caching.
- The public tile service is best effort and has no SLA. A slow or unavailable base map must not remove form state or generated route data; that behavior is covered by TS-403.
- No tile credential is used. The public policy publishes no numeric request quota and forbids load testing, so the repository does not invent either. Pilot and growth request counts are labelled unapproved assumptions.
- The production delivery workflow captures only the deployed application origin, source revision, referrer policy, and tile CSP in `release/provider-production-capture.json`; it omits bodies, session identifiers, precise route data, and non-allowlisted headers. It deliberately makes no automated tile request because the OSM policy permits normal human-driven interactive viewing and prohibits pre-emptive fetching. Actual referrer and provider cache headers therefore require a sanitized human browser capture.
- The current GPX contains contributor attribution but no ODbL/copyright URI. Its notice adequacy is explicitly pending rather than approved.
- The checked-in review deliberately remains `blocked_pending_protected_evidence`. It does not prove that an actual production origin sends an acceptable referrer, exposes a contact channel, honours caching end to end, fits real traffic, or has approved operating cost.

Sources reviewed:

- OpenStreetMap Foundation, [Tile Usage Policy](https://operations.osmfoundation.org/policies/tiles/)
- OpenStreetMap Foundation, [Copyright and License](https://www.openstreetmap.org/copyright/en)
- OpenStreetMap Foundation, [Attribution Guidelines](https://osmfoundation.org/wiki/Attribution_Guidelines)
- OpenStreetMap Foundation, [Produced Work Guideline](https://osmfoundation.org/wiki/Licence/Community_Guidelines/Produced_Work_-_Guideline)
- Open Data Commons, [ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/)

Review result: **BLOCKED for release**. Source-level endpoint, visible attribution, no-prefetch behavior, immutable local routing, failure behavior, cost-model arithmetic, and capture automation pass repository checks. Deployed origin/cache evidence, operator contactability, measured traffic, provider suitability, legal approval, vendor-backed costs, and budget approval remain absent, so TS-509 and the TS-404 provider gate cannot pass.
