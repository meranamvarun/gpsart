# TS-302 outcome and refinement flow handoff

**Version:** 1
**Producer:** TS-302
**Consumers:** TS-304, TS-305 and later browser journey work

## Browser contract

- `screenForWorkflow` is the single mapping from a validated planner-workflow snapshot to the planner, generating, results, coverage, no-match, or dependency-error screen.
- `outcomePresentation` supplies outcome-specific copy and suggestions for every TS-203 terminal classification. Suggestions carry `feasibility: not_evaluated`; the UI says that only a new service search can evaluate them.
- `AppState.lastAttempt` retains the immutable request and validated terminal status for the most recent completed attempt. A refinement changes only the current request, increments its revision, and leaves the prior request and observed result visible in the same browser session.
- Only `provider_unavailable` exposes dependency retry. Retry is a user action, retains the completed job as evidence, copies the unchanged route inputs to a new request revision, and delegates duplicate-click/idempotency handling to `PlannerWorkflow`.
- A newer user refinement disables retry of the older attempt, so retry never overwrites edited inputs. Later stale terminal responses cannot replace a retained higher revision.

Compatibility: completed-attempt retention is in-memory journey state. Durable route storage remains the responsibility of TS-303 after TS-205 supplies complete immutable route snapshots.

## Verification

```text
tsx --test tests/outcome-flow.test.ts tests/app-state.test.ts tests/planner-screen.test.ts
npm run test:unit
npm run test:visual-shell
npm run build
npm run factory:verify
```
