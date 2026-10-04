# ADR-002: Adopt a checksum-pinned local OSM graph for pilot generation

**Status:** Accepted; pedestrian-policy amendment accepted for TS-107
**Date:** 2026-10-04
**Decision owners:** local-graph prerequisite development and independent QA

## Context

ADR-001 rejected the public Valhalla pilot because the route and edge-attribution calls could not prove one immutable traversal. TS-105 remained blocked until a local graph could return ordered geometry, edge identity, distance, access/direction restrictions, and data provenance from the same pinned bytes.

The replacement must not infer bidirectionality, access, connectivity, or a nearby anchor. It also needs a real Bengaluru proof for walking and cycling without sending a user's route to an external service.

## Decision

Adopt `LocalOsmGraph` for the bounded Stage 1 pilot. It loads a checked-in OSM-derived snapshot only when its external SHA-256 manifest matches, validates the snapshot schema plus every retained node/way version, and builds exact OSM-way segment identities. Route geometry and great-circle segment distance are computed from the retained node sequence; ordered occurrences are never collapsed.

Only explicit `foot`, `bicycle`, `access`, `oneway`, surface, and exclusion tags are interpreted. Access is evaluated as a per-profile tri-state: a one-profile denial cannot erase unknown access for the other profile, and unsupported conditional access makes only the affected profile unknown. Missing, profile-specific, or conditional direction remains unknown and is ineligible. Coordinate anchors must equal a retained node exactly; the provider never snaps or silently moves them. The same `AttemptControl` deadline, cancellation, and atomic provider-call budget used by the routing contract remains in force.

The v1 Cubbon Park artifact contains only five connected ways chosen because both `foot=yes` and `bicycle=yes` and `oneway=yes` are present. Walking and cycling proof routes traverse 28 ordered segments across four source ways. A repeated-anchor proof preserves repeated segment occurrences and counts their distance each time. Provenance returns the snapshot ID, derived-artifact SHA-256, and OpenStreetMap/ODbL attribution.

This capability unblocks TS-105 and TS-106 pilot feasibility work. It does not establish Bengaluru-wide coverage or production map-update operations.

## TS-107 pedestrian-policy amendment

The five-way v1 graph was sufficient to prove the adapter but not to benchmark recognizable shape and text traversal. TS-107 therefore adds the append-only `cubbon-park-osm-v2` artifact and `osm-pedestrian-policy-v2`; v1 remains pinned for its original tests. Independent review then found that the v2 text traversal was not recognizable as `HI`, so the benchmark now uses the append-only `bengaluru-central-osm-v3` capture rather than weakening the review criterion.

The v2 policy distinguishes documented OSM pedestrian semantics from guesses. `highway=footway`, `pedestrian`, and `steps` imply pedestrian access unless an explicit or conditional restriction overrides it. A general road `oneway` is a vehicle restriction for the foot profile; pedestrian direction uses `oneway:foot`. A general `oneway` on a path-like way remains ambiguous without `oneway:foot`, and any applicable conditional direction remains unknown and ineligible. Untagged ordinary-road access is still unknown. These rules are pinned separately from `walking-v1` so a mapping-policy change cannot silently reinterpret an immutable route request.

The v2/v3 extractor retains only known-walking-access ways, removes ways participating in turn-restriction relations that the snapshot would otherwise omit, and rejects duplicate coordinates, sub-metre segments, ambiguous path direction, and conditional path direction. The expanded v3 central-Bengaluru capture retains 1,237 ways and 5,291 nodes at derived SHA-256 `df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45`. Candidate acceptance additionally requires a sealed `ValidatedCandidate` with positive access, direction, and turn evidence; unknown access can no longer pass from routing into scoring.

Because map tags and real-world conditions can still be stale or incomplete, `route-use-advisory-v1` requires the route-detail surface to tell users to verify current access and legality and obey signs, closures, property restrictions, and applicable law. That residual-risk advisory does not weaken any application-side hard gate.

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
