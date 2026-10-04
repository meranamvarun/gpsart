# TS-106 curated-text candidate handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-107, TS-205, TS-301  
**Compatibility:** additive Rust routing contract; changing template integrity checks, ordered design-anchor semantics, occurrence roles, diagnostic distance accounting, or no-match/error behavior requires a coordinated versioned handoff

`gpsart-routing` now exposes `CandidateSearch::curated_text` and `curated_text_with_control` for the immutable published `HI@1` template. `CuratedTextSearchRequest`, `CuratedTextSearchConfig`, and `CuratedTextCandidate` are additive aliases of the shared candidate-search contracts so heart and curated text retain identical fixed-start, distance, search-limit, cancellation, and provider-error behavior.

Text fitting preserves the template's stroke and transition order, accepts only its immutable definition hash, enforces the published unmirrored north-up orientation range, and rejects distinct template points that collapse onto one network anchor. Shared endpoints between consecutive template parts are deduplicated without deleting a traversal leg.

The returned candidate retains the canonical full route geometry and every edge occurrence. Occurrence roles distinguish access, artwork, connector, and return travel; `retraced` independently marks repeated edge identities. `connector_distance_metres` and `repeated_distance_metres` are occurrence-derived diagnostics already included in `distance`, never additional totals. `resemblance_trace()` returns the full canonical geometry.

The controlled positive fixture produces the ordered H strokes, explicit H transitions, the inter-letter connector, and the I stroke before returning to the exact confirmed start. Removing the inter-letter edge yields `MatchOutcome::NoMatch`; cancellation remains `CandidateError::Cancelled`, and provider contract failures remain explicit errors.

Verify with `node scripts/cargo.mjs test -p gpsart-routing --test curated_text_candidate`, the complete routing suite, strict routing clippy, and `npm run factory:verify`.
