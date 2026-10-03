# Guest session security policy

Guest sessions are private bearer credentials, not accounts. The API generates 256 random bits and
returns them only in the `gpsart_guest` cookie. Production cookies are `Secure`, `HttpOnly`,
`SameSite=Lax`, and scoped to `/`. The database stores only the SHA-256 digest, constrained to 32
bytes; authentication performs a constant-time digest comparison after lookup. Cookies and API
bodies contain no coordinates, route identifiers, or route geometry.

Sessions last 24 hours. Application startup issues a guest session, and the API supplies an absolute
`rotate_at` timestamp. The browser scheduler calls `POST /api/v1/guest-sessions/current/rotate` with
the readable CSRF cookie at 12 hours, then schedules the replacement policy. The server rejects earlier rotation
with `409 rotation_not_due`; due rotation atomically revokes the old digest and inserts the
replacement. Logout revokes the digest immediately. Expired or revoked sessions cannot
authorize new work. Rows remain only through the cleanup grace period, after which cleanup deletes
the owning session cascade and reports a count—it never selects or logs geometry.

All cookie-authenticated state changes require the exact configured `GPSART_PUBLIC_ORIGIN`, plus a
constant-time match between the readable `gpsart_csrf` cookie and `X-CSRF-Token`. Deployments must
set `GPSART_PUBLIC_ORIGIN` to their canonical HTTPS application origin. The API returns the same
`404 not_found` for a foreign private resource and an unknown identifier. Callers must use the
shared resolved-start/job/route authorizer for start access, status, cancellation, route retrieval,
and GPX export.

Migration 0003 deletes pre-contract token rows whose digest is not exactly 32 bytes before adding
and validating the SHA-256 constraint. This intentionally fails legacy credentials closed while
allowing the schema upgrade to complete safely.
