# TS-205 verification evidence

Verified 2026-10-04.

Acceptance checklist:

1. `CandidateSnapshot::validate` requires ordered GeoJSON, continuous traversal classifications, structured manoeuvres, start/finish derivation, distance fields, quality band, structured qualifications, complete provenance, and request/profile/template/algorithm/routing-data/candidate version pins. Durable publication additionally requires the sealed TS-107 `ScoredCandidate`, derives the snapshot through `CandidateSnapshot::from_scored`, matches geometry/traversals/manoeuvres/duration/legality/quality/provenance to it, and binds request/profile/template/routing-data/algorithm/distance claims to the immutable planning request in the insert query.
2. Invariant tests reject artwork/connector/total contradictions outside the explicit one-metre tolerance. Retraced metres must equal classified retraced traversal distance and are explicitly included in total.
3. Start, finish, and direction markers are derived from stored ordered geometry/traversals. Structured instructions exactly retain the sealed provider manoeuvres, while duration is derived from the pinned profile pace and candidate distance. Rust API and TypeScript decoder tests preserve both visits through a self-intersection in their original order.
4. Surface, access-tag detail, elevation, and provider metadata are not part of this snapshot contract because the sealed TS-107 candidate does not retain evidence for them. Optional provider attribution is absent when unavailable. No default values or free-form instruction text are synthesized.
5. Generation time and every required immutable version appear in the response and strict client decoder.
6. The route endpoint authenticates the guest and owner-scopes lookup independently of job state. Tests prove owned access plus indistinguishable `404` denial for a foreign session.

Executed checks:

- `npm run factory:verify`: passed; Rust formatting, strict clippy, workspace tests/build, factory tests, 51 TypeScript unit tests, visual-shell tests, Sites tests, typecheck, and production build passed.
- Focused `gpsart-routing`, `gpsart-persistence`, and `gpsart-api` workspace tests: passed, including 4 route-snapshot invariant/source tests and the API snapshot test.
- `gpsart-api --test route_snapshots`: 1 passed.
- TypeScript typecheck and 51 unit tests: passed.
- `gpsart-persistence --test migrations`: compiles in the full gate; PostGIS-backed cases, including complete snapshot round-trip/immutability/owner scope, use the established explicit skip path when `DATABASE_URL` is not present.

No real coordinates, credentials, tokens, provider calls, or fabricated routing truth are present in fixtures. Coordinates are deterministic synthetic contract data.
