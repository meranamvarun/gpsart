# TS-305 production-boundary verification

Verified 2026-10-11.

## Executable acceptance checks

1. `tests/visual-shell-regression.test.mjs` rejects timer-created success and browser-fabricated route candidates.
2. `tests/route-candidate-journey.test.ts` proves that a candidate enters the client only after strict snapshot decoding and exact candidate ID, authorized generation-job ID, and request-revision matching. Fetches retain same-origin credentials and fail atomically.
3. `tests/router.test.ts` proves that `welcome` and the screen-review index are unavailable unless the build was made in Vite development mode with `VITE_GPSART_DESIGN_REVIEW=true`. Query strings cannot enable the mode. The review index is visibly labelled and does not simulate backend success.
4. `scripts/scan-production-bundle.mjs` rejects known route-fixture IDs, sample/demo route labels, sample GPX actions, and prototype-success labels. The scanner runs inside `npm run build`, so CI's factory verification cannot publish a bundle that fails it.
5. `tests/production-states.test.ts` renders empty, loading, no-match, dependency-error, route-unavailable, and base-map-unavailable states with no route fixture.

## Results

- Focused production-boundary, router, candidate journey, and bundle scanner tests: passed.
- `npm run test:unit`: 87 tests passed, including the two fixture-free production-state checks.
- `npm run test:visual-shell`: 2 tests passed.
- `npm run build`: passed; the generated `dist/client` production bundle passed the prototype-material scan.
- `npm run factory:verify`: passed, including Rust format/clippy/tests/build, frontend tests, Sites tests, production build, and bundle scan.

The API integration suites independently exercise authorized guest job status and owner-scoped route snapshot endpoints. The browser boundary test exercises the same decoded terminal-job and snapshot contracts without introducing a client fixture path into production code.
