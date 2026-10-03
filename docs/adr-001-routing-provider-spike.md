# ADR-001: Replace the proposed Valhalla pilot integration

**Status:** Accepted  
**Date:** 2026-10-02  
**Decision owners:** TS-104 development and independent QA

## Context

GPSART needs ordered anchors to produce one auditable traversal containing provider edges, geometry, canonical distance, manoeuvres, activity restrictions, and immutable routing-data provenance. A route cannot be accepted when those facts come from different or mutable traversals.

The controlled graph proves the provider-neutral contract, restrictions, repeated edges, cancellation, deadlines, and attempt budgets. The live Valhalla spike returned multi-anchor route shapes, but its route response omitted edges. Exact `edge_walk` attribution failed for the complex outputs, while `map_snap` would be a second routing interpretation. The tested service also lacked immutable snapshot identity and documented operating limits/retention.

## Decision

Do not adopt the tested Valhalla public-demo setup for GPSART candidate generation. Keep `RouteProvider`, `AttemptControl`, `ControlledGraph`, and the strict `ValhallaAdapter` boundary. Evaluate a locally queryable graph built from a checksum-pinned Bengaluru extract behind the same trait.

TS-105 is blocked until that replacement produces reviewed walking and cycling traversals with ordered edge identity and restrictions from the same pinned graph. No car fallback, map-snap substitution, or inferred access is allowed.

## Consequences

- The controlled adapter remains deterministic test infrastructure, not production routing truth.
- Valhalla can be reconsidered only with a pinned local deployment and exact edge attribution for the returned traversal.
- Provider failures remain explicit (`Unavailable`, `Unsupported`, `InvalidProviderResponse`, `NoPath`, cancellation, deadline, and budget exhaustion).
- The replacement must publish provider/data version, attribution, limits, retention, and measured deployment cost before adoption.
