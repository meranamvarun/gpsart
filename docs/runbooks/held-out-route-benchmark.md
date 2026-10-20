# Held-out route benchmark runbook

This runbook defines the fail-closed TS-601 evidence boundary. It does not turn pilot fixtures, performance workloads, or incomplete review records into held-out evidence.

## Roles and sequence

1. A selection owner who does not tune the route algorithm preregisters the selection plan, freezes the case manifest, and records its SHA-256 digest before execution.
2. Independent reference reviewers establish feasibility from complete legal reference traversals on the exact pinned network snapshot. A failed product search is not evidence that a case is impossible.
3. The benchmark operator records one output for every frozen case without deletion or reclassification. Exact coordinates, route geometry, credentials, cookies, tokens, and personal data stay outside the checked-in package.
4. Three independent reviewers receive normalized full-trace artifacts, the same fixed alternatives, and no intended-design label. Each reviewer records one decision for every feasible case. Disagreement is retained.
5. QA runs the verifier, checks the protected raw-output retention record and digests, samples normalized artifacts against raw outputs, and signs only the immutable package that was reviewed.

The selection plan, case manifest, algorithm/configuration artifacts, and review alternatives must be frozen before their respective downstream steps. Changing an algorithm, threshold, case classification, intended design, reference traversal, or review alternatives invalidates the run and requires a new benchmark version with new cases.

## Evidence package

The checked-in JSON package uses schema `gpsart-held-out-benchmark-v2` and contains:

- immutable source, client, API, worker, routing-manifest, map-data, configuration, and algorithm pins;
- at least 50 feasible cases, including at least 20 shape and 20 text cases, plus at least 20 restriction, difficult, or no-match cases;
- provenance, selection rule, opaque reviewed-start reference, intended design, and independently reviewed reference-traversal digest for every case;
- exactly one classified output for every case, including a protected raw-output digest;
- a normalized full-trace artifact and all eight validity results for every returned route;
- exactly three distinct blind and independent review decisions for every feasible output;
- no post-execution exclusions; and
- separately recomputed shape and text totals, qualifying counts, rates, invalid-route count, and structural threshold result.

The package must not contain exact coordinates, trace arrays, route geometry, cookies, tokens, credentials, or passwords. Normalized review SVGs must remove coordinates and metadata while preserving the complete ordered route, including connectors and retracing.

## Verification

Run the contract tests while developing the workflow:

```text
npm run test:benchmark-contract
```

Run the structural preflight for an assembled evidence package with:

```text
npm run verify:benchmark -- --preflight path/to/held-out-benchmark.json
```

The structural preflight fails when any declared case is missing, excluded, duplicated, lacks three declared blind independent decisions, has inconsistent review alternatives, falls below either 80% category threshold, contains an explicitly privacy-forbidden field, reports an inconsistent aggregate, or declares one returned route that fails a required validity check.

Passing the structural preflight never reports or implies that the release gate passed. JSON cannot authenticate its own assertions. Independent QA must separately authenticate the provenance and preregistration time of protected case inputs, reference-traversal feasibility, raw outputs, reviewer identities and independence, timestamps, artifact digests, normalized renderings, production artifact identity, validity evidence, and the final signature. Until that external authentication exists, the CLI always reports `release gate NOT PASSED`.
