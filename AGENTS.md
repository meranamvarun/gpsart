# GPSART code-factory contract

This repository is operated by parallel implementation and QA agents. Read this file, the assigned story, and `factory/guardrails.md` before changing code.

## Work boundaries

- Work on one claimed story (or a named, tightly scoped bug) per branch/worktree. Do not make opportunistic changes outside it.
- A story may be claimed only when `npm run factory:next -- --role dev` reports it as ready. The coordinator resolves blocked work; a developer must not bypass a dependency with a stub.
- Treat checked-in contracts, migrations, templates, and public types as shared interfaces. Coordinate an interface change through a handoff before merging it.
- Keep commits small and story-scoped. Do not overwrite another agent's work. Rebase and resolve conflicts with the owner when files overlap.

## Test-driven delivery

1. Restate the relevant acceptance criteria as executable checks.
2. Add or update a failing test before production code; record an exception in the story evidence only when a test is not practical.
3. Implement the smallest change that passes.
4. Run the story verification commands and the factory checks.
5. Ask a QA agent to independently review the diff and execute the acceptance checklist.

No implementation is complete because a happy path works. Include error, cancellation, timeout, authorization, stale-data, and unknown-data behaviour when the story requires them.

## Non-negotiable product rules

- Never fabricate routing geometry, connectivity, distance, safety/access data, instructions, or success metrics.
- Never silently move a requested start, widen a tolerance, substitute a car route, omit travelled connectors, or hide retracing.
- Preserve exact coordinates, full traces, credentials, cookies, tokens, and personal data from logs, analytics, fixtures, and error messages.
- Use canonical metres and version immutable route inputs/results. Public API changes update OpenAPI, examples, server contracts, and client decoders together.
- Prefer explicit unknown/unavailable states to guessed data. Production prototype fixtures must be build-time-only and visibly labelled.

## Handoffs and learning

- Put a dependency/API handoff in `factory/handoffs/` before another story depends on it. Consumers do not infer unfinished contracts.
- QA rejects a change with missing evidence, untested acceptance criteria, broken gates, or a violated guardrail.
- Record every confirmed agent-caused defect in `factory/incidents.jsonl`. A maintainer turns the root cause into a test, linter/check, or guardrail before closure.
