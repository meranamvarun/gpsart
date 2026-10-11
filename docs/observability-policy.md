# Privacy-preserving observability

GPSART emits JSON `operational_metric` events. The generation-job UUID is also the correlation ID, connecting acknowledgement, queue claim, worker attempt, provider call, and terminal publication without identifying a guest or retaining a coordinate. System maintenance failures use a fresh opaque UUID.

Allowed dimensions are activity, coarse distance bucket, shape/text category, controlled profile/template/map/algorithm versions, and a bounded machine result class. Exact distances, coordinates, requested pins, confirmed starts, route geometry, instructions, cookies, tokens, credentials, custom text, and idempotency keys are prohibited.

The checked-in dashboard and alert definitions in `ops/observability/` are provider-neutral examples. A deployment may translate their query syntax, but it must preserve metric names, thresholds, coarse dimensions, redaction, and the linked response runbook.
