# TS-204 planner workflow handoff

**Version:** 2  
**Producer:** TS-204  
**Consumers:** TS-302, TS-304 and later browser journey work

## Browser contract

- `ProposedStart` is the single input model for manual coordinates, a selected search result, a map click, and explicit browser geolocation. Search integrations dispatch `gpsart:search-selection` with `{ point: { latitude, longitude }, label }`; free text is never geocoded or guessed in the browser.
- `PlannerWorkflow` owns start resolution, explicit confirmation, idempotent creation, conditional polling, cancellation, refresh recovery, and stale-response fencing. Callers supply the current request revision and a CSRF-token reader.
- After confirmation, creation retries reuse the exact serialized request and idempotency key. An ambiguous response is recovered before newer work can start, so a key is never paired with changed resolved-start or route inputs.
- Exactly one accepted or uncertain job is followed at a time. If its request becomes stale, it remains owned and polled to a terminal `previousStatus`; the UI labels progress with that job's immutable request.
- `gpsart.active-job.v1` is a private `sessionStorage` recovery record. It contains the owned job ID, immutable request, proposed start, expiry, and the last validated status representation/ETag. A newly accepted job keeps expiry explicitly unknown until its first authoritative status response. The record is not analytics or cross-session storage.
- `gpsart.guest-policy.v1` in `localStorage` contains only public rotation/expiry policy. Reusing it while secure guest cookies exist prevents refresh from replacing job ownership; no credential is copied into browser storage.
- Only observed job stages are rendered. A terminal status for an older revision is retained as `previousStatus` and cannot replace the current request.

## Shared API/database change

`POST /api/v1/route-jobs` version 1 now requires `return_to_start: boolean`. Migration `0006_planning_request_finish.sql` adds the immutable field. Existing rows remain nullable to represent historical unknown data; all new API-created rows contain an observed boolean. The OpenAPI example, Rust request type, persistence input, browser request, and upgrade tests move together.

Example request fragment:

```json
{
  "requested_distance_metres": 5000,
  "distance_tolerance_metres": 500,
  "return_to_start": true
}
```

Compatibility: clients using the prior request shape receive the existing explicit invalid-request response. No server default is applied.

## Verification

```text
tsx --test tests/start-selection.test.ts tests/planner-workflow.test.ts tests/planner-screen.test.ts tests/guest-session.test.ts
cargo test -p gpsart-api --test route_jobs -- --test-threads=1
cargo test -p gpsart-persistence --test migrations -- --test-threads=1
npm run factory:verify
```
