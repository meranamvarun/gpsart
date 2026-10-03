# TS-103 verification evidence

Verified on 2026-10-03 with deterministic fictional coordinates and controlled providers.

- Domain fixtures cover invalid coordinates, outside coverage, no access, provider failure, short/long snaps, expiry, cross-session access, and changed profile/policy/data versions.
- API contract tests cover versioned GeoJSON, the explicit non-feasibility claim, response fields, authorization, private POST bodies, exact 30 metre confirmation, invalid coordinates, and rejection at 31 metres.
- The TypeScript decoder preserves requested and candidate points separately and rejects unknown versions, invalid coordinates/distances, and any response that bypasses explicit confirmation.
- OpenAPI publishes all new request/response boundaries without putting coordinates in path or query parameters.

No production routing geometry or local feasibility result is included: the provider boundary returns explicit `no_routable_access` or `provider_failure` when truth is unavailable.
