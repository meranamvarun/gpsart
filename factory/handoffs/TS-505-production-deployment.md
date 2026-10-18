# TS-505 production deployment handoff

**Version:** 1  
**Producer:** `codex/root`  
**Consumers:** TS-506, TS-507, TS-508, TS-509, TS-602  
**Compatibility:** additive deployment contract; artifact manifests and environment files are versioned inputs, and promoted image digests are immutable. Database migrations remain forward-only.

## Published contract

- The supported target is Kubernetes on `linux/amd64` with nginx Ingress, cert-manager, External Secrets, an environment-specific managed secret store, and a provider-verified encrypted `encrypted-rwo` StorageClass.
- `scripts/render-deployment.mjs` accepts an environment JSON file plus an artifact manifest containing one full source revision, four digest-addressed images, their SBOM digests, and `platform: linux/amd64`. It fails closed on mutable images, an absent hostname, or storage not explicitly marked encrypted.
- One TLS host terminates at ingress and sends all traffic to `gpsart-client-edge`; that service serves immutable client files and proxies `/api/*` and `/health/*` internally. API cookie and Origin/CSRF policy receives the exact `https://<host>` value.
- Runtime, forward-only migration, and Postgres bootstrap credentials are separate managed-secret records. Application images and rendered source inputs contain no credential values.
- The API exposes HTTP liveness/readiness. The non-public worker readiness command is `/app/gpsart-worker --check`; it verifies the graph, database connection, and current migrations. Process termination is the worker liveness boundary, and both Linux binaries handle SIGTERM through their graceful shutdown paths.
- Delivery builds once, records source/image/SBOM digests, applies foundation then migration then runtime, performs connected staging smoke, and promotes the same manifest after protected production approval. Rollback applies only the runtime phase of a previously approved manifest and never starts an old migrator or reverses migrations.
- Deployment records must omit response bodies, cookies, credentials, precise locations, route geometry, and custom text.

## Verification

```text
node --test tests/deployment-contract.test.mjs
npm run factory:verify
```

Live staging, rollback, backup/restore, monitoring integration, provider review, and public release approval remain explicit environment-owned evidence; source checks must not be represented as those live results.
