# TS-201 guest-session authorization evidence

Verified 2026-10-03 in `story/ts-201`.

## Acceptance checks

- `guest_sessions.rs`: 6 tests pass with two independently issued clients. They inspect `Secure`,
  `HttpOnly`, `SameSite=Lax`, path scope, absence of coordinates, SHA-256-only storage, exact Origin,
  double-submit CSRF, atomic rotation/revocation, expiry, count-only cleanup, and identical `NotFound`
  results for foreign and unknown resolved starts/jobs/routes. Early rotation is rejected and a due
  rotation revokes the previous credential.
- `guest-session.test.ts`: the browser decoder accepts only the public expiry/rotation policy and no
  bearer credential; application startup issues the private cookie and schedules an authenticated,
  CSRF-protected rotation request at the server-provided absolute deadline.
- `persistence-contract.test.mjs`: the forward-only migration set includes the 32-byte digest
  constraint and deletes unusable pre-contract digest rows before validation.
- `cargo clippy -p gpsart-api -p gpsart-persistence --all-targets -- -D warnings`: passed.
- `npm run build && npm run factory:verify`: passed. The explicit prebuild is required because the
  current factory sequence checks generated Sites files before its build step.

The PostgreSQL-backed cleanup operation deletes by session cutoff and returns only a row count. It
does not select, format, or log route geometry. Database-backed migration integration tests remain
conditional on the documented `DATABASE_URL` test environment; the full gate passed with those
tests explicitly skipped when that variable was absent.
