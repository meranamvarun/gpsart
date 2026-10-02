# TS-101 design-template handoff

**Version:** 1  
**Producer:** `codex/dev-ts101`  
**Consumers:** TS-105, TS-106, TS-301 and catalogue clients  
**Compatibility:** published `(id, version)` definitions and hashes are immutable; changes require a new monotonic version

`gpsart-domain::templates` owns normalized template validation and traversal classification. `heart@1` and `HI@1` are source-controlled JSON definitions. The loader rejects malformed/non-finite points, empty strokes, invalid transitions/orientation, text mirroring or reordering, and complexity violations. Every gap between strokes is an explicit `connector` or `retrace` traversal part.

`GET /api/v1/designs` returns published catalogue entries only. The contract is documented in `openapi/gpsart.yaml`, and `src/domain/design-template.ts` validates browser responses. Golden SVG and SHA-256 fixtures under `crates/gpsart-domain/tests/fixtures/` lock the published visuals.

Verify with `npm run factory:verify` and `node scripts/cargo.mjs test -p gpsart-domain --test templates`.

