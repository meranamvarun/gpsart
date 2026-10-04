# TS-205 immutable route-candidate snapshot handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-206, TS-207, TS-303, TS-401  
**Compatibility:** additive migration and API; changing snapshot field meanings, distance tolerance, legality qualifications, ownership behavior, or wire names requires a new coordinated handoff version

`CandidateSnapshot` in `gpsart_persistence::routes` is the complete immutable route result. Producers construct it with `CandidateSnapshot::from_scored` and call `PostgresRouteSnapshotStore::publish` with the same sealed TS-107 `ScoredCandidate`. Publication checks the snapshot's geometry, occurrence classes/directions, structured provider manoeuvres, profile-derived duration, distances, quality band, template/profile/routing-data/routing-policy pins, provenance, and positive access/direction/turn evidence against that sealed source. The database insert additionally joins through the job to its immutable planning request and requires the same request revision, profile, template, routing-data, algorithm versions, and requested distance tolerance. It rejects invalid geometry order, discontinuous traversal classification, contradictory distances, repeated distance that is not marked as included, missing version pins, and unverified legality evidence. The explicit artwork-plus-connector equality tolerance is one metre. Access and return traversals count as travelled connectors for that public distance partition. Repeated distance is a subset of total distance and must never be added again.

Migration `0007_route_candidate_snapshots.sql` adds a one-to-one immutable JSONB snapshot beside the existing immutable `route_candidates` row. Historical candidate rows remain explicitly without a complete snapshot; the migration does not invent missing classifications or metadata. Publishing writes the spatial index row and complete snapshot in one transaction. Surface, access-tag detail, elevation, and provider metadata are excluded because the sealed candidate has no retained evidence for them; optional attribution is omitted when unavailable.

`GET /api/v1/routes/{id}` authenticates the active guest session and queries by `(route id, owner)`. Foreign and nonexistent routes both return `404`; storage or immutable-validation failures return `503`. Authorization does not depend on the source job still being active. The response uses `Cache-Control: private, no-store`. Start, finish, and direction markers derive from the same ordered trace/traversals that supply the map geometry; structured instructions retain the sealed provider manoeuvres, and duration derives from the versioned profile pace and candidate distance.

The response and strict TypeScript decoder are documented in `openapi/gpsart.yaml` and `src/domain/route-snapshot.ts`. Browser persistence encodes that complete wire contract and rejects legacy, partial, reordered, unknown-field, or internally contradictory records. The self-intersection regression fixes coordinate order as supplied; consumers must not simplify or reorder it.

Verify with:

```text
npm run factory:verify
node scripts/cargo.mjs test -p gpsart-persistence --test route_snapshots
node scripts/cargo.mjs test -p gpsart-api --test route_snapshots
npm exec -- tsx --test tests/route-snapshot.test.ts tests/client-boundaries.test.ts
```

PostgreSQL round-trip, owner scoping, and database immutability execute in `gpsart-persistence --test migrations` when `DATABASE_URL` identifies an administrative PostGIS database; otherwise those cases use the repository's explicit skip path.
