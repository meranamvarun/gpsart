# ADR-002: Adopt a checksum-pinned local OSM graph for pilot generation

**Status:** Accepted
**Date:** 2026-10-04
**Decision owners:** local-graph prerequisite development and independent QA

## Context

ADR-001 rejected the public Valhalla pilot because the route and edge-attribution calls could not prove one immutable traversal. TS-105 remained blocked until a local graph could return ordered geometry, edge identity, distance, access/direction restrictions, and data provenance from the same pinned bytes.

The replacement must not infer bidirectionality, access, connectivity, or a nearby anchor. It also needs a real Bengaluru proof for walking and cycling without sending a user's route to an external service.

## Decision

Adopt `LocalOsmGraph` for the bounded Stage 1 pilot. It loads a checked-in OSM-derived snapshot only when its external SHA-256 manifest matches, validates the snapshot schema plus every retained node/way version, and builds exact OSM-way segment identities. Route geometry and great-circle segment distance are computed from the retained node sequence; ordered occurrences are never collapsed.

Only explicit `foot`, `bicycle`, `access`, `oneway`, surface, and exclusion tags are interpreted. Missing or unsupported access remains qualified as unknown. Missing or profile-specific direction remains unknown and is ineligible. Coordinate anchors must equal a retained node exactly; the provider never snaps or silently moves them. The same `AttemptControl` deadline, cancellation, and atomic provider-call budget used by the routing contract remains in force.

The v1 Cubbon Park artifact contains only five connected ways chosen because both `foot=yes` and `bicycle=yes` and `oneway=yes` are present. Walking and cycling proof routes traverse 28 ordered segments across four source ways. A repeated-anchor proof preserves repeated segment occurrences and counts their distance each time. Provenance returns the snapshot ID, derived-artifact SHA-256, and OpenStreetMap/ODbL attribution.

This capability unblocks TS-105 and TS-106 pilot feasibility work. It does not establish Bengaluru-wide coverage or production map-update operations.

## Operating boundary

| Concern | Pilot result |
| --- | --- |
| Limits | 40 nodes, 5 ways, 40 directed segments, exact retained-node anchors only. Two- and four-anchor requests are covered. `AttemptControl` supplies wall-time and provider-call limits. |
| Latency | The seven focused loader/routing tests, including both real-profile proofs, complete in 0.01 seconds on the development machine after compilation. This is an observation, not an SLA. |
| Attribution | Every route carries `OpenStreetMap contributors (ODbL 1.0)`; downstream UI/export must retain it. |
| Caching | The graph is parsed once by its owner and held in-process. There is no remote response cache. |
| Retention | The artifact contains public OSM data only. Route queries are evaluated in memory; this component does not log or persist user coordinates. |
| Deployment cost | No routing service, network call, or paid provider. The checked-in JSON is 11,010 bytes; CPU/memory cost is in-process and was not separately priced. |
| Data updates | Manual, append-only snapshot revision with new capture time, element versions, source checksum, derived checksum, review, and benchmark. Mutable latest-data replacement is forbidden. |

## Consequences

- TS-105 may search only within the pinned graph and must return no candidate when it cannot fit a legal route.
- A later coverage router may replace this implementation behind `RouteProvider`, but must meet the same immutable traversal and provenance contract.
- OSM turn-restriction relations touching selected ways may not be omitted. The v1 extractor fails if any such relation exists; the v1 selected ways have none.
- Unsupported conditional/profile-specific rules remain unknown rather than guessed. Production expansion requires explicit parsing and new fixtures before those edges become eligible.
