# TS-105 legal heart-candidate handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-106, TS-107, TS-205  
**Compatibility:** additive Rust routing contract; changing occurrence ordering, distance accounting, closure validation, or no-match/error semantics requires a coordinated versioned handoff

`gpsart-routing` now exposes `CandidateSearch::heart` over the existing `RouteProvider` plus an `AnchorMatch` boundary. `heart_with_control` additionally accepts a caller-owned `AttemptControl` for end-to-end cancellation. It projects immutable normalized template points into a local metric frame, enumerates configured placement/scale/rotation variants inside the template orientation bounds, matches only network anchors within the configured tolerance, and sends the confirmed start, ordered design anchors, and confirmed return as one routing query.

`MatchOutcome::Match` contains the immutable template identity/hash, exact request profile, full routed geometry, ordered design anchors, every routed edge occurrence, canonical repeated distance, measured closure, and provider/data provenance. Occurrences retain access/artwork/connector/return roles and independently flag repeated edge identities. `NoMatch` is used for a legal but unrealizable search or exhausted search limits. Cancellation and provider/contract failures remain explicit `CandidateError` values and are not misreported as no-match.

Candidates pass hard validation before return: exact first point, closure at most five metres without appended geometry, contiguous complete edge coverage, a checked edge-distance sum equal to provider distance, unrounded request bounds, ordered leg boundaries that exactly equal requested network anchors, verified provider access, and retained-memory limits. The wall deadline and cancellation are checked during fitting as well as provider work. `ControlledGraph` accepts exact point anchors for deterministic parity with `LocalOsmGraph`; neither graph snaps a route query.

Verify with `node scripts/cargo.mjs test -p gpsart-routing --test heart_candidate`, the complete routing suite, strict routing clippy, and `npm run factory:verify` using the GNU Rust toolchain documented by the repository.
