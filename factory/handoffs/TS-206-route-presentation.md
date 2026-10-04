# TS-206 route comparison and details handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-207, TS-304  
**Compatibility:** changing candidate selection state, comparison-label rules, routed-geometry semantics, fallback states, or the required advisory requires a coordinated handoff revision

The browser fetches every successful job result ID from `GET /api/v1/routes/{id}` with same-origin credentials and `no-store`, strictly decodes the TS-205 immutable snapshot, and installs the set atomically. A missing, unavailable, malformed, ID-mismatched, or request-revision-mismatched snapshot produces an explicit unavailable/invalid state; the UI does not show a partial comparison.

`AppState.routeResults`, `candidateLoad`, `candidateRequestRevision`, and `selectedCandidateId` are the presentation boundary. Stale loads cannot replace a newer revision. Candidate selection changes both the selected card and the exact ordered trace/direction-marker model consumed by Leaflet. The comparison remains north-up. Full alternative traces are muted; the selected trace is segmented by the immutable traversals, with connector/return/retraced travel visually distinguished. A tile failure changes only the base-map status: routed vectors and the text facts remain available.

`buildCandidateComparison` accepts at most the first three distinct decoded geometries. “Closest distance” minimizes absolute distance from the immutable request target. “Best shape match” ranks the displayed set by public quality band, then displayed artwork share, then displayed repeated share; route ID is only the stable final tie-break. No ideal outline is generated or used as routing geometry, and the UI explicitly says the optional ideal design overlay is not shown.

The detail surface renders total, artwork, connector, and repeated distance (repeated is labelled as included in total), endpoint policy, duration/profile, quality band, every traversal class/direction, structured instructions without invented prose, provider/attribution, version provenance, and the versioned `route-use-advisory-v1` text.

Verify with:

```text
npm exec -- tsx --test tests/route-presentation.test.ts tests/route-candidate-journey.test.ts
npm run test:unit
npm run build
npm run factory:verify
```
