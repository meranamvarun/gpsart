# TS-404 first-release verification

Decision: **NO-GO**, recorded 2026-10-11.

The release checklist is traceable and deliberately fail-closed. The repository does not contain the required independently reviewed 50-case held-out benchmark or a signed manual journey across all five target browsers. The corrected TS-403 workload also misses the 30-second p95 termination target and 60-second hard maximum. No recognition percentage, route-validity pass over the missing benchmark, or browser pass has been inferred.

The evidence assembly did uncover and correct two provider-obligation defects: the raster client used the obsolete subdomain URL form and plain-text attribution, and GPX export discarded snapshot attribution. Contract tests now enforce the exact official tile endpoint, linked copyright attribution, and retained GPX attribution. Independent QA still marks the provider gate blocked because no deployed origin, traffic/caching record, operator contact channel, derived-data publication review, or production cost evidence exists.

## Evidence map

- Signed decision and gates: `docs/release/first-release-checklist.json`
- Held-out gap: `docs/evidence/ts-404-held-out-benchmark.json`
- Browser gap: `docs/evidence/ts-404-browser-matrix.json`
- Provider review: `docs/release/provider-obligations.md`
- Limitations: `docs/release/limitations.md`
- Performance and recovery: `docs/evidence/ts-403-pilot-performance.json` and `docs/evidence/ts-403-recovery-performance-verification.md`
- Accessibility: `docs/evidence/ts-304-accessibility-verification.md`
- Security/privacy: `docs/evidence/ts-401-workload-retention-privacy-verification.md` and `docs/evidence/ts-402-observability-verification.md`
- GPX external-import evidence: `docs/evidence/ts-207-gpx-export-verification.md`

Independent QA signed the NO-GO checklist on 2026-10-12. TS-404 can be complete as an evidence-assembly activity; it does not authorize a product release while any release gate is blocked or failed.
