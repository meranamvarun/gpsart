# TS-601 held-out benchmark blocker

Recorded: 2026-10-20  
Story result: **BLOCKED**

TS-601 cannot be completed from the frozen production-candidate artifacts currently checked in. The canonical TS-404 benchmark record contains zero independently reviewed feasible cases, zero shape cases, zero text cases, zero difficult or no-match cases, and zero blind reviewers. Its recognition rates and returned-route validity result are therefore unknown.

The available evidence cannot be substituted:

- TS-107 contains two calibrated pilot cases (`heart@1` and `HI@1`). They were used during implementation and are explicitly not held out.
- TS-506 is a 20-job performance workload. Its retained result is aggregate-only (10 succeeded, 9 no-match, and 1 cancelled) and contains neither independently reviewed reference traversals nor blind recognition decisions.
- The approved production manifest exposes two reviewed starts for walking and running. Cycling is explicitly unreviewed.

Repeating the known pilot cases, selecting only successful TS-506 inputs after observing their outcomes, or inventing reviewer decisions would violate the anti-leakage criterion and the repository rules against fabricated routing truth and success metrics.

## Implemented benchmark boundary

The repository now includes `scripts/verify-held-out-benchmark.mjs`, the `gpsart-held-out-benchmark-v2` workflow in `docs/runbooks/held-out-route-benchmark.md`, and focused contract coverage in `tests/held-out-benchmark-contract.test.mjs`. This is deliberately a structural preflight: it rejects inconsistent declarations and recomputes category thresholds, but it never authenticates evidence or reports that the release gate passed. `npm run factory:verify` executes this contract suite.

This tooling closes only the structural validation gap. It cannot create or authenticate the independently established routing truth, artifacts, reviewer identities, or decisions required to execute and pass the release gate.

## Required external inputs and follow-up work

1. Before algorithm or threshold changes, preregister and freeze a privacy-safe set with at least 50 independently reviewed feasible cases (including at least 20 shape and 20 text cases) plus at least 20 restriction, difficult, or no-match cases. Retain provenance, selection rules, independently reviewed reference traversals, and immutable artifact pins.
2. Implement a runner that retains every submitted outcome and fails closed on continuity, exact confirmed start, access/direction/turn policy, requested distance tolerance, connector/retrace disclosure, immutable version consistency, authorization, or GPX round-trip failure.
3. Produce identity-hidden review packets with fixed design alternatives and collect decisions from three independent reviewers for every feasible output, retaining disagreement.
4. Reproducibly aggregate shape and text recognition rates separately and obtain an independent QA signature over the complete package.

## Verification and test exception

`node --test tests/release-evidence-contract.test.mjs` passed 3/3 on 2026-10-20 and confirms that the missing benchmark remains an explicit fail-closed release blocker. With the repository's local PostGIS test service configured, `npm run factory:verify` also passed in 95.1 seconds, including the mandatory connected database cases.

The contract suite uses explicitly labelled, in-memory synthetic values only to test the preflight's rejection and aggregation logic. Such values can exercise structural preflight, but can never produce an authenticated release-gate pass. The first real execution input—the preregistered independently reviewed held-out set—still does not exist. The independent QA readiness review reached the same blocked verdict on 2026-10-20.

After INC-080, independent QA adversarially verified that embedded top-level or aggregate pass claims fail structural preflight, the public assessment always keeps `release_gate_passed` false, and the CLI explicitly reports `release gate NOT PASSED`. The final focused benchmark suite passed 7/7, and the final database-backed `npm run factory:verify` passed in 64.3 seconds on 2026-10-20.
