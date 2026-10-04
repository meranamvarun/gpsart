# TS-105 heart-candidate verification

## Acceptance trace

The focused `heart_candidate` suite translates the story criteria into deterministic checks against the immutable `heart@1` template and the existing routing boundary:

1. The successful route geometry begins and ends at the exact confirmed network point. Closure is measured from routed geometry and must be at most five metres; no closing coordinate is appended.
2. Every returned edge occurrence is retained in order and labelled as access, artwork, connector, or return. Repeated edge identities are marked `retraced`; the fixture's return repeats both access edges.
3. Candidate distance is the checked sum of every routed edge occurrence, including repeated travel, and is compared directly with the inclusive, unrounded metre interval.
4. Empty, truncated, discontinuous, distance-inconsistent, out-of-range, over-memory, deadline-exhausted, provider-access-unverified, and provider-rejected paths cannot become candidates. Every provider via point must equal its requested network anchor. Provider contract failures remain errors rather than false no-matches.
5. Placement, scale, rotation, anchor-match, provider-call, retained-point, and wall-time limits are explicit configuration and are enforced during both fitting and routing. Rotations outside the immutable template bounds are rejected. An externally supplied `AttemptControl` supports cancellation throughout fitting and routing.
6. The controlled legal fixture returns a recognisable heart while the out-of-tolerance fixture returns `NoMatch`, with no direct-line fallback.

## Deterministic result and validation trace

- Template: `heart@1`, with its immutable definition hash carried into the candidate.
- Profile: `walking-v1`, copied exactly from the request into the candidate.
- Ordered route: confirmed start -> two access occurrences -> eight heart-outline occurrences -> the same two access edges in reverse as return occurrences.
- Distance: 120 metres, equal to the sum of all 12 routed occurrences and inside the requested 100–130 metre interval.
- Closure: 0 metres at the confirmed network point.
- Provenance: `controlled-graph`, data version `heart-fixture-v1`.
- Negative fixture: the only placement is outside the configured anchor tolerance, producing explicit `NoMatch` without calling a straight-line fallback.

The complete route is rendered in `ts-105-heart-route-preview.svg`. The blue access and dashed purple return are intentionally visible; the red routed outline preserves the lower point, two upper lobes, and central indentation of `heart@1`.

## Verification

```powershell
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
node scripts/cargo.mjs test -p gpsart-routing --test heart_candidate
node scripts/cargo.mjs test -p gpsart-routing -- --test-threads=1
node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings
npm run factory:verify
```

Developer result on 2026-10-04 before review: the focused suite passed 4/4, the complete routing suite passed 28/28, strict routing clippy passed, and the unchanged `npm run factory:verify` gate passed after a production build populated `dist`. The first factory run reproduced the documented pre-existing command-order issue: `test:sites` ran before `build` in the fresh worktree and returned nonzero; the same run then built successfully, and the unchanged canonical rerun passed every phase.

Independent QA then found validation gaps despite the green gates: unverified provider access was accepted, orientation bounds were not enforced, fitting could outlive its deadline, via points were not reconciled with requested anchors, and provider `InvalidRequest` was misclassified as no-match. Regressions were added before each correction. The revised focused suite passes 7/7 and the complete routing suite passes 31/31; it now also exercises external and provider cancellation explicitly. The final independent rerun is recorded by QA below.

## Final independent QA

Reviewer `codex/qa-ts105` approved the revised implementation at `e2064a3` on 2026-10-04. The full story diff and rendered SVG evidence were inspected against every acceptance criterion and the routing guardrails. The review confirmed that the requested start and complete traversal remain explicit, repeated travel is retained and counted, illegal or structurally inconsistent routes are rejected, immutable orientation and bounded-search limits are enforced, provider-access uncertainty cannot be published as legal, cancellation and provider failures remain explicit, and unrealizable geometry returns `NoMatch` without a straight-line fallback.

Independent commands and results:

- `node scripts/cargo.mjs test -p gpsart-routing --test heart_candidate`: 7 passed.
- `node scripts/cargo.mjs test -p gpsart-routing -- --test-threads=1`: 31 passed.
- `node scripts/cargo.mjs clippy -p gpsart-routing --all-targets -- -D warnings`: passed with warnings denied.
- `npm run factory:verify`: passed unchanged, including workspace formatting, clippy, tests, frontend checks, Sites checks, and production build.
