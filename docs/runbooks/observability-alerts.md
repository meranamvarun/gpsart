# Observability alert runbook

All investigation uses an opaque correlation/job UUID. Never copy request bodies, coordinates, route geometry, cookies, credentials, or custom text into logs, tickets, or dashboards.

## Queue backlog

Check worker readiness and deployment concurrency first. Preserve the queue and its immutable inputs; do not raise release caps to clear it. Scale only within the reviewed deployment limit.

## Stuck leases

Confirm worker health and database time. Let expired leases follow the fenced retry path. Never edit lease ownership or publish a partial candidate manually.

## Provider failures

Check provider status, quota, attribution, and the server-side credential configuration. Failed calls remain failed/unknown; do not substitute another travel mode or fabricated geometry.

## Error rate

Group only by public result class and version dimensions, then follow opaque correlation IDs. If a version regressed, stop that version rather than exposing private request data for diagnosis.

## Timeout rate

Compare queue, search, and provider latency percentiles with the hard 60-second runtime. Timeouts stay explicit and must not be relabelled as no-match or success.

## Cleanup failure

Check database readiness and the retry state in `cleanup_runs`. The 100-family batch and active-lease exclusion remain mandatory during recovery.
