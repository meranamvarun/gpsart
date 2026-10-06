# TS-303 on-device saved-route handoff

**Version:** `saved-route-record-v1`
**Producer:** TS-303
**Consumers:** TS-304, TS-305
**Status:** independently QA-approved
**Compatibility:** schema changes, request/snapshot validation changes, or account-coupled behavior require a coordinated versioned handoff

`IndexedDbSavedRouteRepository` in `src/client/saved-routes.ts` owns the `gpsart-saved-routes` IndexedDB database and its `routes` object store. Each independently keyed record contains the user-visible name, save timestamp, complete immutable `RouteRequest`, and complete encoded TS-205 `RouteSnapshot`. A record is accepted only when both contracts decode strictly and their request revisions and route IDs agree.

Listing isolates malformed records and unknown schema versions as per-record errors, so valid routes remain usable. Save, rename, and delete use read-write IndexedDB transactions. Storage denial, quota exhaustion, and other failures are explicit results; no operation changes the currently selected server route or its GPX export path.

Reopening installs only the stored request and snapshot into app state, clears unrelated job-attempt state, and never calls routing or route-detail APIs. Offline GPX export passes that stored snapshot directly to the TS-207 serializer. Authentication state is absent from the database contract: signing in or out cannot upload, erase, or relabel these records.

The saved-routes UI must retain the exact phrase “Saved on this device,” explain that clearing site data removes saves, and keep per-record failures visible without hiding valid records.

Verification:

```powershell
npm exec -- tsx --test tests/saved-routes.test.ts tests/gpx-export.test.ts tests/route-candidate-journey.test.ts
npm run typecheck
npm run factory:verify
```
