# TS-101 template verification

Verified 2026-10-02.

- Published inputs: `heart@1` and `HI@1` in `crates/gpsart-domain/templates/`.
- Deterministic rendered evidence: heart and HI SVG fixtures plus stable SHA-256 files in `crates/gpsart-domain/tests/fixtures/`.
- Focused Rust tests cover immutable versions, malformed coordinates/strokes/joins/orientation/complexity, explicit transitions, golden render/hash stability, and published-only catalogue filtering.
- TypeScript decoder tests reject drafts, bad hashes, and unknown categories.
- `npm run factory:verify` is the required complete gate.

