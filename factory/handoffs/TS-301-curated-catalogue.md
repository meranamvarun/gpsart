# TS-301 curated-catalogue handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-305, TS-404 and catalogue clients  
**Compatibility:** the `GET /api/v1/designs` response remains published-only and adds the required `accessible_name` field; changing required summary fields, published identities, or the fail-closed client behaviour requires a coordinated handoff

`GET /api/v1/designs` returns only reviewed, immutable published templates and retains `shape` / `text` category filtering. Each summary contains stable template identity and version, `published` status, category, display name, accessible name, complexity hint, SVG preview data, and definition hash. Catalogue presence is not evidence of feasibility at a particular start.

The first-release published set is deliberately limited to `heart@1` and `HI@1`, the only templates with legal positive and relevant negative controlled fixtures plus recognisability evidence. Circle, triangle, star, square, GO, and RUN are not embedded or accepted by route-job creation. The browser keeps those prototype choices visible but disabled with a specific no-reviewed-fixture reason.

The browser fetches and decodes the catalogue before enabling generation. Network failure, malformed data, a category mismatch, a missing identity/version, or an unpublished prototype all fail closed without clearing start, activity, distance, tolerance, or return-to-start values. Selected controls use `aria-pressed`; disabled controls use `disabled` and `aria-describedby`.

Verification is recorded in `docs/evidence/ts-301-catalogue-verification.md`. Run `npm run factory:verify` for the complete gate.
