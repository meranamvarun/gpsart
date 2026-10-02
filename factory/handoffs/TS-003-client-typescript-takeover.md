# TS-003 takeover handoff

**Status:** implementation complete; ready for independent QA.  
**Producer:** `codex/root`  
**Last verified:** 2026-10-02  
**Base commit:** `5683e9d`

## Completed and verified

- React, React DOM, and the React Vite plugin were removed. Vite builds all five HTML entry points through `src/main.ts`.
- Strict TypeScript is enabled by `tsconfig.json`; `npm run build` runs `npm run typecheck` first.
- Typed domain/client modules exist for planner requests, immutable route snapshots, API decoding, browser storage, routing, and the design catalogue.
- The retained visual shell is now rendered by native typed modules through `src/main.ts` importing `src/app.ts`; the legacy `src/app.js` controller was removed.
- The live Shapes/Text catalogue now imports typed previews from `src/app/catalogue.ts` without visual change.
- Evidence captures are in `docs/evidence/ts-003-desktop-planner.png` and `docs/evidence/ts-003-mobile-planner.png`. They show labelled illustrative prototype data only.
- `npm run factory:verify` passes at this commit with the workspace-local Node runtime in `.tools/node-v22.15.0-win-x64` when the system runtime is absent.

## Published module contracts

| Module | Contract | Consumers | Compatibility |
| --- | --- | --- | --- |
| `src/domain/planner.ts` | `RouteRequest`, revision changes, distance bounds, stale-result comparison | planner controller | metres must remain canonical at backend boundaries; UI currently uses kilometres only for display/input |
| `src/domain/route-snapshot.ts` | immutable `RouteSnapshot` decoder | `src/client/api.ts`, `src/client/storage.ts` | rejects malformed positions, revisions, and distances; do not loosen validation |
| `src/client/api.ts` | `decodeApiRouteResult` discriminated API result | future TS-204 browser client | no API response enters app state before decoding |
| `src/client/storage.ts` | versioned `gpsart.route-snapshots.v1` records | saved-route controller | malformed/legacy values must return `unsupported_snapshot`, never crash startup |
| `src/app/catalogue.ts` | typed design preview catalogue | active `src/app.ts` | preserves Heart/Circle/Triangle/Star/Square and HI/GO/RUN |

## Original takeover checklist (completed; pending QA confirmation)

1. Port the active visual screen structures from `src/app.js` into typed modules one-for-one: rendering, router, state, map, storage, API boundary, and screen controllers. Preserve desktop and 360px layouts rather than replacing them with placeholder panels.
2. Make the typed planner form model the only route-affecting browser state. Route results must use request revisions and reject stale results.
3. Remove or isolate the legacy controller’s generated sample candidate geometry, metrics, timer-driven success path, and sample GPX. The running screen code must not fabricate route geometry, connectivity, distance, safety/access data, instructions, or success metrics.
4. Keep the current map/design fixture only if it is build-time-only and visibly labelled as illustrative; never present it as navigable output.
5. Wire saved-route rendering to `loadRouteSnapshots`/`saveRouteSnapshots`, preserving the explicit unsupported-snapshot state.
6. Recapture desktop and 360px screenshots from the final typed implementation, update story evidence, and request an independent QA review.

Completion notes: `AppState.request` is the sole route-affecting model; request edits clear prior results and decoded snapshots are accepted only at the current revision. Generated candidates, metrics, GPX, and timer-driven success were removed. The map fixture is labelled illustrative, and malformed saved data produces an explicit unsupported state. Final desktop and true 360 px captures were regenerated on 2026-10-02.

## Important QA finding

QA rejected commit `6dfe39c` because `src/app.ts` replaced the existing visual catalogue/cards/maps/auth/saved/detail layouts with minimal placeholder panels. The active bootstrap was restored in `639668e`; `tests/visual-shell-regression.test.mjs` protects that baseline until a visually equivalent typed port replaces it. Update that test only together with replacement screenshots and QA evidence.

## Safe implementation order

1. Move the existing presenter constants/templates into typed modules with focused tests, as done for `catalogue.ts`.
2. Extract typed planner state and handlers while keeping the existing HTML and CSS output byte-for-byte/visually equivalent.
3. Extract each screen controller and map adapter; compare against the baseline captures at 1440px and 360px.
4. Replace prototype result creation with explicit unavailable/no-match/error states until TS-201 through TS-205 deliver actual jobs and snapshots. Do not bypass those dependencies with a client-side substitute.
5. Delete the legacy controller only after the typed bootstrap has the complete visual layout and all checks pass.

## Verification

```powershell
$nodeRoot = Join-Path $PWD '.tools\node-v22.15.0-win-x64'
$env:Path = "$nodeRoot;$env:Path"
& (Join-Path $nodeRoot 'npm.cmd') run factory:verify
```

Run focused `npm run test:unit` after each extraction. QA must independently run `npm run factory:verify`, review screenshots/diff, update the TS-003 QA record, and only then mark the story `done`.
