# Monitoring and incident response

The `gpsart-oncall` team owns application response. `platform-oncall` owns the cluster, database, monitoring pipeline, and notification delivery; `engineering-lead` is incident commander and approves closure. Use only the opaque correlation UUID, attempt number, provider-call ordinal, controlled version labels, and public result class. Never paste request URLs or query strings, bodies, cookies, authorization or CSRF headers, coordinates, route geometry, full traces, custom text, credentials, or tokens into dashboards, pages, tickets, chat, or error-reporting tools.

## Triage

Acknowledge the page, record alert name, start/end time, environment, release revision, and coarse metrics, then confirm the alert from a second privacy-safe signal. Search restricted application logs only by the opaque correlation UUID returned to the browser as `x-correlation-id`. Do not broaden logging during an incident.

## Escalation

Escalate application alerts from `gpsart-oncall` to `platform-oncall` after ten minutes or immediately for database, monitoring, or notification failure. Escalate to `engineering-lead` for multi-component impact, suspected privacy exposure, or mitigation requiring traffic or release changes.

## Mitigation

Prefer the smallest reversible action: stop promotion, restore a healthy runtime revision, scale within reviewed limits, or disable new admissions with a truthful unavailable/retry response. Never change route inputs, loosen tolerances, substitute routing modes/providers, publish partial candidates, edit lease ownership, or relabel unknown/failure outcomes.

## Closure

Confirm the triggering query is healthy for two full evaluation windows, notification delivery works, queued work reached truthful terminal states, and cleanup resumed. Record timestamps, opaque correlations only, mitigations, and follow-up owners. A suspected privacy exposure remains open until the privacy owner completes review.

## API unavailable

Check edge and API readiness, the current deployment revision, and database readiness. Stop promotion and roll back runtime only when the forward-only migration compatibility check permits it.

## Acknowledgement latency

Compare API saturation, database latency, and admission-control outcomes. Keep the one-second p95 gate; use bounded 429 responses rather than accepting work that cannot be serviced.

## Queue backlog

Check worker readiness and reviewed concurrency. Preserve immutable queued inputs and do not raise release caps solely to clear the queue.

## Stale worker or stuck lease

Check worker readiness, lease renewal, and database time. Let the fenced retry path recover expired leases; never edit lease ownership or manually publish partial work.

## Provider failures

Check provider health, quota, attribution, and server-side credential availability. Keep failures explicit and never substitute a different mode or fabricated geometry.

## Error rate

Group only by public result class and controlled versions. Stop a regressed version; do not inspect or attach private route inputs.

## Timeout rate

Compare queue, search, and provider percentiles with the 60-second hard limit. Keep timeouts explicit rather than relabelling them as no-match or success.

## Cleanup failure

Check database readiness and `cleanup_runs`. Preserve bounded batches, active-lease exclusion, and retention limits. Page remains open until a subsequent cleanup succeeds and overdue data is absent.

## Monitoring system outage

Use Kubernetes readiness and direct privacy-safe health probes while the pipeline is unavailable. Notify `platform-oncall` through the independent deadman path. Do not enable raw request logging as a substitute.

## Alert delivery failure

Confirm `alertmanager_notifications_failed_total`, test the independent deadman receiver, and contact `platform-oncall` out of band. After repair, inject the controlled delivery test and record both firing and resolved notifications without message payloads from users.
