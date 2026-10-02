# TS-002 migration verification

Verified on 2026-10-02 with the workspace-local Node and Rust GNU toolchains.

- `npm run test:persistence`: 3 static persistence-contract checks passed; the 6 Rust migration tests compiled and executed.
- `npm run factory:verify`: passed; this covers Rust formatting, Clippy with warnings denied, workspace tests/build, frontend tests, type checking, and the production build.
- The checked-in CI job provisions `postgis/postgis:16-3.4`, supplies `DATABASE_URL`, and runs the same workspace tests against that service.

This Windows host has no Docker, `psql`, `pg_ctl`, or PostgreSQL service and `DATABASE_URL` is unset. Consequently, the Rust migration test functions returned through their explicit no-database skip path here; this local run does **not** claim that clean-install or upgrade SQL executed against PostGIS. The CI service is the database-backed acceptance environment.

The database-backed cases create isolated databases and verify:

1. clean application of every embedded migration, PostGIS availability, expected tables, and SRID 4326 geometry;
2. upgrade from migration 0001 to the current schema and the complete constrained job-state set;
3. startup rejection for missing and checksum-incompatible migration state.
4. cross-owner request and route-candidate foreign-key rejection;
5. deferred rejection of outcomes on active jobs and terminal jobs without an outcome, plus commit of one matching outcome;
6. immutable request/candidate rejection and uniqueness of owner-scoped idempotency keys and job-scoped candidate versions.
