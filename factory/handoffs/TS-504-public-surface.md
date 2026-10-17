# TS-504 public-surface handoff

**Version:** 1

**Producer:** TS-504

**Consumers:** TS-602 and release verification

**Compatibility:** intentionally removes unsupported authentication routes and the browser place-search bridge

## Contract

- The production public screens are planner, catalogue, generating, results, details, saved, no-match, coverage, and error. The design-review index remains development-build-only.
- `login`, `auth-pending`, `auth-error`, `welcome`, and `account` hashes or HTML fallbacks resolve to the guest planner. Query parameters and development review mode do not change that result.
- No Google authentication control, callback action, or place-name search event bridge is exposed by the client.
- The planner and catalogue identify the Bengaluru pilot, Heart and HI as the calibrated first-release designs, service-verified exact coverage, and the lack of arbitrary-start or custom-text promises.
- Saved routes remain on the current device, are not uploaded or synced, and are removed when site data is cleared.
- Production emitted text assets are rejected when they contain the removed sign-in/search paths or the former citywide/search-result claims.

## Verification

```text
npm exec -- tsx --test tests/public-surface.test.ts tests/router.test.ts tests/saved-routes.test.ts tests/accessibility.test.ts
node --test tests/production-bundle.test.mjs tests/visual-shell-regression.test.mjs
npm run build
npm run test:a11y
npm run factory:verify
```

Evidence and the signed scope record are in `docs/evidence/ts-504-public-surface-verification.md` and `docs/release/first-release-public-scope.md`.
