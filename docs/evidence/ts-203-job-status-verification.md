# TS-203 job-status verification

Verified 2026-10-03 on `story/ts-203`.

- API tests cover authenticated owner-scoped reads, private not-found behavior, durable ETags,
  exact and wildcard conditional requests, cache policy, absence of invented percentage progress,
  every terminal classification, and retry guidance.
- TypeScript decoder tests cover active and terminal lifecycle consistency, every public outcome
  class, result IDs, exact fields, UUIDs, timestamps, revisions, budget flags, and retry guidance.
- Persistence transition tests cover monotonic observed stages across lease recovery, idempotent
  stage observation, owner isolation, exact immutable terminal result IDs, atomic terminal
  classification, and rejection of mismatched or migration-only worker outcomes.
- Migration `0005_job_status.sql` adds the monotonic status version and constrained public
  classification. Conditional PostgreSQL coverage verifies owner-scoped reads and the complete
  queued-to-terminal transition and a populated-v4 upgrade when `DATABASE_URL` is available; it
  skips explicitly otherwise.
- The OpenAPI contract includes active status plus an example for every supported terminal result.
  It documents `200`, `304`, private `404`, authentication failure, and storage failure responses.

No route geometry, precise location, provider result, percentage, or success metric is fabricated
by this story. Stages and terminal classifications are published only from durable observations.
