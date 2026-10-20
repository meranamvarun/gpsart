# TS-509 provider-obligations handoff

**Version:** 1

**Producer:** `codex/root`

**Consumers:** TS-602, TS-603

**Compatibility:** additive provider-review, cost-model, capture-artifact, and workflow contracts; no public API or route format changes.

## Published contract

- `ops/providers/production-review.json` is the canonical source review. Consumers must reject any state other than an evidence-backed accountable approval. Its checked-in state is `blocked_pending_protected_evidence`.
- `ops/providers/monthly-cost-model.json` contains reproducible pilot and planning-growth arithmetic. Its rates are unapproved allowances, not vendor prices or budget authority; all three accountable decisions remain pending.
- Production delivery runs `scripts/capture-provider-review.mjs` only after deployment and retains `release/provider-production-capture.json`. The capture is bound to a full source revision and records only allowlisted application-origin headers. It makes no automated tile request; provider referrer/cache evidence must come from a sanitized human browser journey.
- The origin artifact is necessary but insufficient. TS-509 also requires the human browser capture, measured traffic, a public contact channel, provider suitability, accountable legal review and any ODbL 4.6 offer required before public route display/export, vendor-backed cost inputs, and engineering/platform/finance approvals.
- Provider outage, throttle, quota, or delay never substitutes a routing mode. Form state and generated route data remain intact while the UI reports an explicit unavailable or delayed dependency state.

## Consumer rule

TS-602 and TS-603 must remain blocked while the review status, licensing review, cost assumptions, or approvals are pending. They must not infer approval from a passing source test, a locally generated artifact, or the existence of the deployment capture step.

## Verification

```text
npm run test:provider
npm run factory:verify
```
