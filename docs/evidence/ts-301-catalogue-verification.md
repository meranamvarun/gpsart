# TS-301 curated catalogue verification

Verified 2026-10-06.

## Release decision

The first-release catalogue publishes only `heart@1` and `HI@1`. Both immutable templates have schema, controlled-routing, negative, golden-render, and pilot recognisability evidence. Circle, triangle, star, square, GO, and RUN remain visible prototype choices but are disabled: none has the required legal positive routing fixture and recognisability review, so publishing any of them would claim unsupported capability.

Catalogue membership explicitly does not claim feasibility for a user's current start. The routing service evaluates that only after the user submits the preserved planner inputs.

## Fixture matrix

| Template | Schema and immutable preview | Legal positive fixture | Relevant negative fixtures | Recognisability review | Decision |
| --- | --- | --- | --- | --- | --- |
| `heart@1` | `published_hashes_and_svg_renders_are_golden` | `feasible_fixture_is_closed_legal_recognisable_and_complete` | no-match network; incomplete/illegal path; rotation bounds; cancellation, provider error, deadline and search limits | Pinned pilot trace in `docs/evidence/ts-107-pilot-heart.svg` | Published |
| `HI@1` | `published_hashes_and_svg_renders_are_golden` | `feasible_hi_is_one_complete_ordered_full_trace_with_honest_diagnostics` | disconnected graph; mirrored/reordered/rotated input; collapsed anchors; incomplete/reordered provider trace; cancellation and limits | Pinned pilot trace in `docs/evidence/ts-107-pilot-hi.svg` | Published |
| `circle@1` | No reviewed template | No legal fixture | Not reviewed | Not reviewed | Disabled |
| `triangle@1` | No reviewed template | No legal fixture | Not reviewed | Not reviewed | Disabled |
| `star@1` | No reviewed template | No legal fixture | Not reviewed | Not reviewed | Disabled |
| `square@1` | No reviewed template | No legal fixture | Not reviewed | Not reviewed | Disabled |
| `GO@1` | No reviewed template | No legal fixture | Not reviewed | Not reviewed | Disabled |
| `RUN@1` | No reviewed template | No legal fixture | Not reviewed | Not reviewed | Disabled |

## Acceptance evidence

1. `crates/gpsart-api/tests/designs.rs` checks category filtering, version, published status, accessible name, and SVG preview data. OpenAPI and the TypeScript decoder require the same fields.
2. The fixture matrix above links every published entry to positive and negative routing checks. The complete routing suites remain the authoritative fixture evidence.
3. `tests/catalogue.test.ts` and `tests/planner-screen.test.ts` prove that unpublished prototypes are disabled with a reason; malformed, unavailable, or identity-inconsistent catalogue data fails closed; and the selected control exposes `aria-pressed`.
4. The catalogue screen says that presence never claims local fit. No catalogue response or browser code creates route feasibility data.
5. `tests/app-state.test.ts` proves a design revision preserves start, activity, and distance. The disabled state preserves the current form and prevents unsupported submission.

## Verification commands

```text
npm exec -- tsx --test tests/catalogue.test.ts tests/design-template.test.ts tests/app-state.test.ts tests/planner-screen.test.ts
node scripts/cargo.mjs test -p gpsart-api --test designs
node scripts/cargo.mjs test -p gpsart-domain --test templates
node scripts/cargo.mjs test -p gpsart-routing --test heart_candidate --test curated_text_candidate
npm run factory:verify
```
