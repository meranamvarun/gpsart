# TS-106 curated-text candidate verification

The deterministic `hi-fixture-v1` graph fits the immutable published `HI@1` template at a fixed placement, scale, and north-up rotation. The accepted route starts and finishes at the exact confirmed point and retains the complete access, artwork, connector, retraced, and return traversal.

## Acceptance evidence

1. `feasible_hi_is_one_complete_ordered_full_trace_with_honest_diagnostics` asserts all eight distinct ordered design anchors, an exact closed full trace, every edge occurrence, and the immutable template identity.
2. Template hash verification plus tests for mirroring, stroke reordering, rotation outside the published ±15° range, collapsed anchors, incomplete provider boundaries, and reordered boundaries fail before a candidate is returned.
3. The 626 m route total equals the sum of every occurrence. Connector distance is a 146 m diagnostic and repeated distance is a 70 m diagnostic; neither is added again to the total.
4. `HeartCandidate::resemblance_trace` exposes the canonical full routed geometry, including access, connectors, retracing, and return, rather than an ideal or artwork-only outline.
5. Removing the only inter-letter network connector produces `MatchOutcome::NoMatch`; no straight-line fallback or partial geometry is returned.

The reviewed trace rendering is [`ts-106-hi-route-preview.svg`](./ts-106-hi-route-preview.svg). Orange dashed segments are template connectors and purple dashed overlays identify repeated traversal occurrences.

## Verification commands

```text
node scripts/cargo.mjs test -p gpsart-routing --test curated_text_candidate
node scripts/cargo.mjs test -p gpsart-routing
node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings
npm run factory:verify
```
