# TS-103 coverage and start-resolution handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-105, TS-202, TS-204  
**Compatibility:** additive API and domain contract; changing outcome codes, the 30 metre ceiling, or version binding requires a new contract version

## Published contract

- `GET /api/v1/coverage` returns versioned GeoJSON and the explicit `local_route_feasibility: not_evaluated` state.
- `POST /api/v1/starts/resolve` accepts the exact requested point only in its private body and returns that point separately from the candidate network point.
- `POST /api/v1/starts/{resolution_id}/confirm` accepts only a resolution whose snap distance is at most 30 metres.
- Both start endpoints require `x-gpsart-guest-session`; a resolution is bound to that opaque session identifier, exact activity-profile version, coverage-policy version, map-data version, and expiry.
- Machine outcomes are `invalid_coordinates`, `unsupported_coverage`, `no_routable_access`, `provider_failure`, `expired_resolution`, `wrong_session`, `profile_changed`, `coverage_policy_changed`, `data_version_changed`, `snap_too_far`, and `authorization_required`.
- `ConfiguredStartService` is the adapter boundary. A deployment supplies a `StartResolverProvider`; absence or provider failure stays explicit and never fabricates a routable point.

Exact coordinates must remain in request/response bodies and private persistence. Do not add them to public URLs, logs, analytics, error messages, or identifiers.

Verify with `cargo test -p gpsart-domain --test start_resolution`, `cargo test -p gpsart-api --test starts`, `npm run test:unit`, and `npm run factory:verify`.
