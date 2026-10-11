# TS-404 release evidence handoff

- Producer: `codex/root`
- Consumer: release owner and independent QA
- Decision: `NO-GO`

The canonical machine-readable decision is `docs/release/first-release-checklist.json`. It links every release gate to checked-in evidence and lists the three release blockers. `docs/release/limitations.md` must remain adjacent to any later decision.

Releasing requires a new, independently signed checklist after the held-out benchmark, five-browser manual journey, and performance gates pass or are explicitly re-authorized by the accountable release owner. Passing repository tests does not convert this NO-GO into a release approval.

Verify with:

```text
npm run test:factory
node scripts/cargo.mjs test -p gpsart-api --test route_snapshots
npm run factory:verify
```
