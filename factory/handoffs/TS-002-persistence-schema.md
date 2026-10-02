# TS-002 persistence schema handoff

**Version:** 1  
**Producer:** `codex/dev-ts002`  
**Consumers:** TS-103, TS-201, TS-202, TS-401 and all services that persist route work  
**Compatibility:** forward-only SQLx migrations; additive changes require a new numbered migration

## Published contract

- `migrations/0001_foundation.sql` owns guest sessions, resolved starts, and immutable planning
  request snapshots.
- `migrations/0002_route_jobs.sql` owns jobs, terminal outcomes, attempts, immutable candidate
  versions, and cleanup metadata.
- All private graph edges enforce guest ownership with composite foreign keys.
- All spatial values are typed SRID 4326 geometry. All distance inputs and outputs are integer metres.
- Consumers must transition a terminal job and insert its one matching outcome in the same
  transaction. The check is deferred until commit.
- Consumers must insert new request/candidate versions; update triggers reject mutation.
- `PostgresProbe` is a startup gate only. Deployments call `apply_migrations` separately and then
  start application processes.

No consumer may infer route geometry, replace exact traces, widen tolerances, or persist plaintext
session credentials. See `docs/persistence-entities-and-retention.md` for GeoJSON conversion and
retention boundaries.

Verify with `npm run test:persistence` and `npm run factory:verify`. With `DATABASE_URL` set to an
administrative PostGIS database, the Rust migration tests create isolated temporary databases and
exercise both a clean install and an upgrade from migration 0001.
