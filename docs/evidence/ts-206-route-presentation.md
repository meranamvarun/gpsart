# TS-206 verification evidence

Verified 2026-10-05 against decoded immutable `route-candidate-snapshot-v1` fixtures.

- One, two, and three distinct candidate sets render exactly once, with exact IDs and no placeholders.
- Selection changes the exact full ordered trace and direction markers while retaining the north-up convention.
- Best-shape and closest-distance labels are derived from the displayed candidates and public metrics.
- Complete distance partitions, retracing, endpoint policy, directions, instructions, provenance, and the route-use advisory render as text independent of map tiles.
- A simulated base-map failure retains route facts and clearly reports only the base map as unavailable.
- Exact result-ID fetches are decoded atomically; missing, malformed, mismatched, and stale responses cannot produce partial or stale geometry.
- The route map explicitly identifies routed geometry and states that the ideal design overlay is not shown.
- Independent QA regressions now prevent an older completion from beginning a load over a newer candidate revision, deduplicate identical ordered traces even when traversal classifications differ, classify malformed successful JSON as invalid, and exercise the production Leaflet tile-error handler.

Automated checks:

```text
npm exec -- tsx --test tests/route-presentation.test.ts tests/route-candidate-journey.test.ts
# 12 passed

npm run test:unit
# 60 passed

npm run build
# passed

npm run factory:verify
# passed (Rust fmt/clippy/test/build, factory, 60 unit, visual-shell, Sites, and production build)
```
