# First-release public product scope

**Record version:** 1

**Story:** TS-504

**Recorded:** 2026-10-17

**Decision:** This scope describes the public surface only; it does not change the repository's NO-GO release decision.

## Included public capabilities

- Anyone may use the planner as a private guest without creating an account.
- Starts are supplied by exact latitude/longitude, an explicit map click, or browser geolocation initiated by the user.
- The public pilot is identified as the Bengaluru pilot. The routing service remains authoritative for exact coverage and routable-access decisions.
- Heart and HI are the only calibrated first-release designs. Catalogue presence does not promise that either design is feasible at every requested start or distance.
- Validated route snapshots may be saved only on the current device and exported as GPX. Clearing site data removes those saved routes; they are not uploaded or synced.

## Excluded public capabilities

- Google sign-in, accounts, cloud synchronization, and authentication callbacks are not shipped.
- Place-name search and geocoding are not shipped.
- Citywide coverage, feasibility at an arbitrary start, arbitrary text, and uncalibrated catalogue designs are not promised.
- A stale URL, client state, query parameter, keyboard path, or design-review flag does not enable any excluded capability in a production build.

## Enforcement and evidence

- The router accepts only the guest planner, catalogue, generation/outcome, route-detail, and device-saved-route screens. Legacy authentication URLs fall back to the guest planner even when query parameters or review mode are supplied.
- The application has no authentication action or place-search event bridge.
- The production-bundle scanner rejects Google sign-in labels, the former search bridge, citywide claims, and place-search copy.
- Automated accessibility and visual evidence covers 360 px, 768 px, and 1440 px planner layouts plus catalogue, saved-route, and result surfaces. The audit also checks keyboard access and legacy authentication URL fallbacks.

## Developer attestation

I, `codex/ts504-dev`, attest that the TS-504 implementation and evidence preserve this scope and do not claim unsupported routing truth or release readiness.

Signed: `codex/ts504-dev`

Date: 2026-10-17

Independent QA approval is recorded separately in `factory/stories.json`; QA, not the developer, decides whether the story may advance from `in_review` to `done`.
