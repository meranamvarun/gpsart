# TS-302 outcome and refinement verification

## Acceptance checks

1. `tests/outcome-flow.test.ts` drives every validated TS-203 terminal classification through destination selection and rendered copy. Bounded no-match text never claims impossibility, and each suggested change is visibly labelled as not locally evaluated.
2. Start-resolution coverage/access failures reach the coverage view, dependency failures reach the error view, and correctable field errors remain with the planner.
3. `tests/app-state.test.ts` proves start, activity profile, design, distance, tolerance, and return-to-start changes each create a new request revision. Category-only UI state remains outside the route request.
4. Refine-after-no-match and refine-after-success tests prove the completed request, classification, and observed result references keep their original inputs while the current request advances. A concurrent newer polling request cannot be displayed under the retained outcome.
5. Provider retry is available only for the server-classified retryable outcome. It creates one new revision from unchanged inputs, retains the completed job, refuses to overwrite later edits, and relies on the TS-204 logical-request idempotency boundary for repeated clicks.

## Verification results

- Focused outcome/state/screen suite: 10 passed.
- Complete TypeScript unit suite: 48 passed.
- Visual-shell regression suite: 2 passed.
- Typecheck and production build: passed.
- Factory verification: passed with the repository's documented `stable-x86_64-pc-windows-gnu` toolchain, including Rust format, clippy, all workspace tests/builds, factory contracts, browser tests, Sites tests, and production build.
