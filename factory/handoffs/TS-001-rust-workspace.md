# TS-001 Rust workspace handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-002, TS-101, TS-102, TS-201 and later Rust services  
**Compatibility:** additive workspace/crate APIs; consumers must not introduce framework dependencies into `gpsart-domain`

## Published boundaries

| Crate | Contract |
| --- | --- |
| `gpsart-domain` | framework-independent IDs, canonical `Metres`, and activity values |
| `gpsart-config` | validated environment configuration, redacted secrets, JSON tracing initialization |
| `gpsart-persistence` | `DependencyProbe`, redacted PostgreSQL readiness, stable `postgres_unavailable` code |
| `gpsart-routing` | provider-neutral `RouteProvider` boundary |
| `gpsart-api` | Axum router/server with `/health/live`, dependency-aware `/health/ready`, graceful shutdown |
| `gpsart-worker` | dependency check followed by shutdown-aware work loop; no lease begins after shutdown |

Required API environment variables are `GPSART_DATABASE_URL` and `GPSART_API_BIND`. The worker requires only `GPSART_DATABASE_URL`. No secret has a default, and secret values must be accessed explicitly through `SecretString::expose` only at the dependency boundary.

Example unavailable readiness response:

```json
{"status":"unavailable","code":"postgres_unavailable"}
```

Verify with `npm run factory:verify`. Local Windows execution automatically uses the ignored workspace toolchain when present; CI uses its native Rust installation.

