# Guest session security policy

Guest sessions are private bearer credentials, not accounts. The API generates 256 random bits and
returns them only in the `gpsart_guest` cookie. Production cookies are `Secure`, `HttpOnly`,
`SameSite=Lax`, and scoped to `/`. The database stores only the SHA-256 digest, constrained to 32
bytes; authentication performs a constant-time digest comparison after lookup. Cookies and API
bodies contain no coordinates, route identifiers, or route geometry.

Sessions last 24 hours. Clients rotate at 12 hours using
`POST /api/v1/guest-sessions/current/rotate`; rotation atomically revokes the old digest and inserts
the replacement. Logout revokes the digest immediately. Expired or revoked sessions cannot
authorize new work. Rows remain only through the cleanup grace period, after which cleanup deletes
the owning session cascade and reports a count—it never selects or logs geometry.

All cookie-authenticated state changes require the exact configured `GPSART_PUBLIC_ORIGIN`, plus a
constant-time match between the readable `gpsart_csrf` cookie and `X-CSRF-Token`. Deployments must
set `GPSART_PUBLIC_ORIGIN` to their canonical HTTPS application origin. The API returns the same
`404 not_found` for a foreign private resource and an unknown identifier. Callers must use the
shared job/route authorizer for status, cancellation, route retrieval, and GPX export.
