# TS-103 coverage and start-resolution handoff

**Version:** 2
**Producer:** `codex/root`  
**Consumers:** TS-105, TS-202, TS-204  
**Compatibility:** additive API and domain contract; changing outcome codes, the 30 metre ceiling, or version binding requires a new contract version

## Published contract

- `GET /api/v1/coverage` returns versioned GeoJSON and the explicit `local_route_feasibility: not_evaluated` state.
- `POST /api/v1/starts/resolve` accepts the exact requested point only in its private body and returns that point separately from the candidate network point.
- `POST /api/v1/starts/{resolution_id}/confirm` accepts only a resolution whose snap distance is at most 30 metres.
- In the integrated API, both start endpoints require the TS-201 opaque guest-session cookie plus
  exact Origin and double-submit CSRF protection. A resolution is bound to the authenticated
  session ID, exact activity-profile version, coverage-policy version, map-data version, and expiry.
  The `app_with_starts` constructor retains its private test-only header harness so TS-103 domain
  outcomes can be exercised independently; production uses `app_with_services`.
- Machine outcomes are `invalid_coordinates`, `unsupported_coverage`, `no_routable_access`, `provider_failure`, `storage_unavailable`, `expired_resolution`, `wrong_session`, `profile_changed`, `coverage_policy_changed`, `data_version_changed`, `snap_too_far`, and `authorization_required`. Confirmation persistence failures use `storage_unavailable` with HTTP 503 and are never reported as routing-provider failures.
- `ConfiguredStartService` is the adapter boundary. A deployment supplies a `StartResolverProvider`; absence or provider failure stays explicit and never fabricates a routable point.
- Production supplies `PostgresResolvedStartStore`; confirmation succeeds only after the exact requested and confirmed points plus owner, expiry, profile, coverage-policy, and map-data versions are durably written. Storage failure is `storage_unavailable` (HTTP 503).
- The API binary requires `GPSART_COVERAGE_VERSION`, `GPSART_COVERAGE_BOUNDS` (`min_lon,min_lat,max_lon,max_lat`), `GPSART_MAP_DATA_VERSION`, and `GPSART_ROUTABLE_STARTS_JSON`. Each configured network point names the exact profile versions for which the pinned snapshot marks it routable; startup rejects missing, empty, or malformed configuration.

Exact coordinates must remain in request/response bodies and private persistence. Do not add them to public URLs, logs, analytics, error messages, or identifiers.

Verify with `cargo test -p gpsart-domain --test start_resolution`, `cargo test -p gpsart-api --test starts`, `npm run test:unit`, and `npm run factory:verify`.
