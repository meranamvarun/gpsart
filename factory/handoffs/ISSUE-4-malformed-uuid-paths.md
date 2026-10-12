# Issue 4 malformed UUID path error contract

- Version: `malformed-path-error-v1`
- Producer: `gpsart-api`
- Consumers: TypeScript planner workflow, route-candidate client, all HTTP API clients
- Compatibility: Additive OpenAPI response. Valid UUID behavior is unchanged. A malformed UUID path now returns JSON instead of Axum's undocumented plain text.

## Contract

Every public operation whose path contains a UUID returns HTTP `400` with `Content-Type: application/json` and exactly this body when that identifier is malformed:

```json
{"code":"invalid_path_parameter"}
```

Covered operations:

- `GET /api/v1/route-jobs/{job_id}`
- `DELETE /api/v1/route-jobs/{job_id}`
- `GET /api/v1/routes/{route_id}`
- `GET /api/v1/routes/{route_id}/export.gpx`
- `POST /api/v1/starts/{resolution_id}/confirm`

Authentication, CSRF, service, and storage behavior for syntactically valid UUIDs is unchanged. The envelope deliberately carries no rejected value or parser detail.

## Verification

```text
cargo test -p gpsart-api --test malformed_uuid_paths -- --test-threads=1
npm run test:unit
npm run typecheck
npm run factory:verify
```
