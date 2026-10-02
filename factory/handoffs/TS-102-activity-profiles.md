# TS-102 activity profile handoff

**Version:** 1  
**Producer:** `codex/dev-ts102`  
**Consumers:** TS-103, TS-104, TS-105, TS-106, TS-205 and later request/candidate APIs  
**Compatibility:** additive Rust domain/routing contract; changing any version identifier or eligibility result requires a new profile version

## Published contract

- `gpsart_domain::profile_for` publishes `walking-v1`, `running-v1`, and `cycling-v1`.
- `RouteRequestProfile::new(activity)` is the request-side identity. `RouteCandidateProfile::from_request(&request)` copies the exact identity to a candidate.
- Both identities serialize as `{ "activity": "...", "profile_version": "...-v1" }`. Decoding rejects mismatched activity/version pairs.
- `gpsart_routing::evaluate_edge` and `evaluate_turn` return either a precise rejection reason or eligibility with explicit unknown-data qualifications.
- `RouteQuery.profile` carries `RouteRequestProfile`; consumers must not pass an unversioned activity alone.
- Canonical distance remains `Metres`. `estimated_duration` is the only calculation that uses pace.

The complete matrix and missing-data behavior are documented in `docs/activity-profile-policy.md`. In particular, unknown access is qualified rather than called verified, while unknown direction and unknown turn permission are rejected.

## Compatibility example

```json
{"activity":"cycling","profile_version":"cycling-v1"}
```

Do not infer a profile version from the activity when reading persisted or provider data. Reject an absent, unknown, or mismatched version at the boundary.

## Verification

```powershell
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-gnu'
npm run factory:verify
```
