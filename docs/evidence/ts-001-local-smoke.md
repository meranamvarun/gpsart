# TS-001 local verification evidence

Verified on 2026-10-02 with Rust 1.99.0 and Node 22.15.0.

- `npm run factory:verify`: passed, including rustfmt, Clippy with warnings denied, all Rust workspace tests, all frontend tests, and both production builds.
- API binary smoke: with PostgreSQL intentionally unavailable, `GET /health/live` returned `200` with `{"status":"ok","code":"live"}` and `GET /health/ready` returned `503 Service Unavailable`.
- API graceful-stop integration: the real Axum server returned cleanly when its shutdown future completed, within the one-second test deadline.
- Worker binary smoke: the worker started without API configuration, checked PostgreSQL, exited with the structured `DependencyUnavailable` error, and did not print its connection string.
- Worker shutdown integration: dependency checks ran once and no work lease was taken after shutdown was already requested.
- CI definition: `.github/workflows/ci.yml` runs formatting, Clippy, workspace tests, the frontend/factory gate, and a separate Gitleaks job. A hosted CI run requires publishing the commit and is not claimed here.

