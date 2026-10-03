# TS-201 guest-session authorization handoff

**Version:** 1  
**Producer:** `codex/dev-ts101`  
**Consumers:** TS-202, TS-203, TS-204, TS-205, TS-207, TS-401  
**Compatibility:** additive API and persistence contracts; changing cookie names, digest algorithm,
session lifetime, ownership denial, or CSRF rules requires a new handoff version

## Published contract

- Production must call `serve_with_auth` with `PostgresGuestSessionStore` and a `SessionPolicy`.
  `GPSART_PUBLIC_ORIGIN` is required and must be the canonical HTTPS application origin.
- `POST /api/v1/guest-sessions` issues `gpsart_guest` (256 random bits, Secure, HttpOnly,
  SameSite=Lax) and `gpsart_csrf` (Secure, SameSite=Strict). Only the SHA-256 guest-token digest is
  persisted. Neither cookie contains coordinates or resource IDs.
- Sessions last 24 hours. Clients schedule the request from the response's absolute `rotate_at` and
  rotate after 12 hours at `POST /api/v1/guest-sessions/current/rotate`; earlier requests receive
  `409 rotation_not_due`. Rotation revokes the old digest and inserts the new
  digest in one PostgreSQL transaction. `DELETE /api/v1/guest-sessions/current` revokes explicitly.
- Cookie-authenticated mutations require an exact Origin match and a constant-time match between
  `gpsart_csrf` and `X-CSRF-Token`.
- Consumers authorize private data through `authorize_resource` / `GuestSessionStore::owns_resource`.
  `PrivateResourceKind::ResolvedStart` covers confirmed starts, `Job` covers
  request/status/cancellation, and `Route` covers route/GPX access.
  Foreign and nonexistent resources both return `AuthError::NotFound`; never disclose ownership.
- Expired and revoked sessions return `Unauthenticated` before work authorization. Cleanup returns
  only `CleanupReport { deleted_sessions }` and never materializes route geometry.
- Migration `0003_guest_session_digest.sql` enforces a 32-byte SHA-256 digest. It is forward-only.

API examples and response schemas are in `openapi/gpsart.yaml`; browser decoding is in
`src/domain/guest-session.ts`. The detailed rotation and CSRF policy is in
`docs/guest-session-security.md`.

Verify with:

```text
npm run build
npm run factory:verify
```
