# TS-305 production boundaries handoff

**Version:** 1  
**Producer:** TS-305  
**Consumers:** TS-404 and release verification  
**Compatibility:** additive build gate; internal client call signature tightened

## Contract

- Production routing results come only from `fetchRouteCandidates`, using the exact terminal job ID, request revision, and ordered result IDs returned by the authorized job-status response.
- Every route snapshot must pass the immutable snapshot decoder and match all three authorization bindings before any candidate state is accepted.
- The prototype review index and unconnected welcome screen are accessible only in a Vite development build with `VITE_GPSART_DESIGN_REVIEW=true`. URL and query parameters cannot enable the mode. Review screens are visibly labelled.
- Every production build scans emitted text assets for known fixture IDs and prototype success labels. A finding fails the build.
- Empty, loading, no-match, dependency-error, and unavailable-map states do not depend on candidate fixtures.

## Verification

```text
npm run factory:verify
```

Focused checks:

```text
npm exec -- tsx --test tests/router.test.ts tests/route-candidate-journey.test.ts tests/production-states.test.ts
node --test tests/production-bundle.test.mjs tests/visual-shell-regression.test.mjs
npm run build
```
