# GPSART code factory

The factory is the coordination layer for delivering the backlog safely with many agents. It is intentionally file-based and deterministic: it runs locally and in CI without a central service.

## Roles

| Role | Responsibility | May mark done? |
| --- | --- | --- |
| coordinator | assigns ready work, manages dependencies and handoffs | no |
| dev | implements one claimed story test-first | no |
| qa | independently validates acceptance criteria and regression risk | yes, after evidence is complete |
| maintainer | triages incidents and adds durable prevention | no |

## Everyday flow

1. Run `npm run factory:next -- --role dev` to find unblocked work.
2. Claim the selected story in `factory/stories.json`, naming the owner and branch/worktree. Commit that claim first.
3. Write tests, implement the story, and complete its evidence fields.
4. Add a handoff file if another story needs a new contract, schema, fixture, or API.
5. A QA agent runs `npm run factory:verify` plus story-specific checks, records its review in the story, and only then sets `done`.

`npm run factory:status` shows dependency-aware state. `npm run factory:verify` validates the coordination data, guards against unacknowledged incidents, and runs the repository checks.

## States and dependency rules

`todo` → `claimed` → `in_review` → `done`. `blocked` is used only with a documented external reason. A story is ready when every listed dependency is `done`; ranges are expanded in the manifest to keep this machine-checkable.

Do not claim work merely because its predecessor is in review. A consumer can prepare tests against a handoff contract, but cannot merge an implementation that relies on an unfinished dependency.

## Feedback loop

When a defect is found, append a structured record to `incidents.jsonl` with its story, root cause, and prevention. The incident remains `open` until its prevention has landed and its `preventionCheck` passes. `factory:verify` fails when an open incident is older than the configured grace period, preventing recurring mistakes from becoming institutional knowledge only in prose.
