# TS-103 verification evidence

Verified on 2026-10-03 with deterministic fictional coordinates and controlled providers.

- Domain fixtures cover invalid coordinates, outside coverage, no access, provider failure, short/long snaps, expiry, cross-session access, changed profile/policy/data versions, and profile-filtered configured network points.
- API contract tests cover versioned GeoJSON, the explicit non-feasibility claim, response fields, authorization, malformed bodies, private POST bodies, exact 30 metre confirmation, invalid coordinates, rejection at 31 metres, and every required stale/error status and code.
- TypeScript decoders cover coverage, successful resolution, successful confirmation, and every recognized machine outcome; they reject unknown versions/outcomes, invalid coordinates/distances, and any response that bypasses explicit confirmation.
- OpenAPI publishes all new request/response boundaries without putting coordinates in path or query parameters.

No production routing geometry or local feasibility result is included: the provider boundary returns explicit `no_routable_access` or `provider_failure` when truth is unavailable.

The full factory gate passed before the QA revisions. After the revisions, formatting, Clippy, all TS-103 focused suites, Rust build, frontend tests, type checking, and production build passed; the workspace Rust-test phase was interrupted only when Windows Security refused to launch the unchanged `profile_policy` test executable with OS error 225. No test assertion failed.
