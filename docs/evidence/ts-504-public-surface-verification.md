# TS-504 public-surface verification

Verified by the developer on 2026-10-17.

## Executable acceptance checks

1. `tests/public-surface.test.ts` verifies the Bengaluru pilot and Heart/HI calibration copy, absence of authentication and place-search activation in the production controller, guest planner availability, and device-only saved-route language.
2. `tests/router.test.ts` verifies that legacy authentication URLs, fallback page state, query parameters, and review mode all resolve to the guest planner.
3. `tests/production-bundle.test.mjs` and `scripts/scan-production-bundle.mjs` reject Google sign-in surfaces, the former place-search bridge, citywide claims, and place-search copy in emitted text assets.
4. `scripts/accessibility-audit.mjs` exercises the production build at 360 px, 768 px, and 1440 px, audits catalogue/saved/results at mobile width, checks keyboard navigation, and proves stale authentication URLs show the planner with no authentication action.
5. Existing saved-route tests continue to prove that route records remain device-only, survive unrelated sign-in state changes, and tell users that clearing site data removes them.

The tests were added before the implementation. The first focused run failed five of seven TypeScript checks and the new bundle-scanner assertions, demonstrating the previous sign-in route, search bridge, citywide copy, and missing scope copy.

## Results

- Focused public-surface/router/saved-route/accessibility suites: 21 passed.
- Production-bundle and visual-shell focused suites: 4 passed.
- `npm run typecheck`: passed.
- `npm run test:unit`: 94 passed.
- `npm run build`: passed; emitted production assets passed the hardened bundle scan.
- `npm run test:visual-shell`: 2 passed.
- `npm run test:a11y`: passed at 360/768/1440 px with no serious or critical axe findings on the audited surfaces.
- `npm run factory:verify`: passed in 166.9 seconds against PostgreSQL after the rebased worktree cache was warmed; all 18 migration checks, production worker scenarios, Rust checks, 94 client unit tests, visual-shell, Sites, and production build/bundle checks passed. Independent QA repeated the complete database-backed gate in 83.6 seconds.
- Mobile and desktop screenshots were visually inspected: navigation remains reachable, no authentication control appears, and the scope copy is readable without overflow.

Independent QA (`codex/qa_ts504_final`) approved all five acceptance criteria on 2026-10-18 after inspecting the final diff and screenshots and independently running the focused 21-test public-surface suite, 4 bundle/visual checks, production build, accessibility audit, and complete factory gate. No blockers or unsupported claims were found.

## Evidence

- `docs/release/first-release-public-scope.md`
- `docs/evidence/ts-504-public-surface-report.json`
- `docs/evidence/ts-504-public-surface-mobile-360.png`
- `docs/evidence/ts-504-public-surface-tablet-768.png`
- `docs/evidence/ts-504-public-surface-desktop-1440.png`

The repository remains NO-GO under the existing release decision. This story hardens public claims and reachability; it does not supply the pending production routing data, deployment, provider obligations, held-out benchmark, performance gate, or five-browser manual journey.
