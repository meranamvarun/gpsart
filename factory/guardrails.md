# Factory guardrails

## Required checks

- Run `npm run factory:verify` before requesting review.
- Add focused automated tests for changed behavior. Keep fixtures deterministic and free of real precise locations, credentials, and tokens.
- New API behaviour requires contract examples and client decoding tests. Database changes require forward-only migration and upgrade coverage.
- QA must run the affected test suite independently, inspect the diff, and record the result in `factory/stories.json`.

## Parallel-work rules

- One agent owns each story at a time; one story is not split across agents without a written subtask contract.
- A shared contract change gets a handoff file with version, producer, consumers, compatibility, examples, and verification command.
- Do not edit another claimed story's files unless its owner explicitly coordinates the change in the handoff.

## Stop-and-escalate conditions

- An acceptance criterion conflicts with a design or API contract.
- A prerequisite is not complete or its handoff is missing.
- A test requires fabricated routing truth, unsafe data handling, or an unreviewed production credential.
- A failing test is unrelated to the story or a required check cannot run.

Escalate with evidence; do not disable the test, weaken the constraint, or silently change product behaviour.
