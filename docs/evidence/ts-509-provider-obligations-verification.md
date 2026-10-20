# TS-509 provider licensing, capacity, and cost verification

Verified at source level on 2026-10-20. This record does not claim a live production deployment, observed production traffic, provider approval, legal advice, a vendor quote, or budget authority.

## Acceptance checks implemented

1. The canonical review binds the exact public tile endpoint and pinned local-routing graph to attribution, credential class, origin/referrer configuration, contact status, user-agent behavior, and cache behavior. Unknown deployed observations stay `pending`; they are not copied from source configuration and called observed.
2. Pilot and planning-growth tile volumes are reproducible assumptions. The OSM public service publishes no numeric quota and was not load-tested. Release remains blocked until an accountable reviewer decides the expected and measured traffic is suitable; an unsuitable decision requires the public tiles to be disabled until a paid, hosted, or self-hosted replacement is approved.
3. Existing TS-403 failure evidence covers a slow/absent base map, provider deadline, and quota failure. The review requires form and generated-route preservation, explicit unavailable/delayed states, and no automatic routing-mode substitution for outage, throttle, quota, or slowness.
4. The licensing record covers ODbL source data, the derivative routing database, route-export working classification, in-product and GPX notices, and a fail-closed publication plan. Public route display/export remains blocked until accountable review decides and implements any ODbL 4.6 database-or-alterations offer. The current GPX has contributor text but no ODbL URI, so its notice adequacy remains pending.
5. The cost model covers compute, storage, backup, bandwidth, tile provider, monitoring, and support for pilot and growth planning. Every total is reproducible; every rate, tier, and owner decision is explicitly unapproved pending provider/host selection, quotes, traffic measurement, and budget approval.
6. Production delivery has a revision-bound, 15-second-bounded origin capture step. It verifies the deployed referrer policy and tile CSP and writes only allowlisted observations. It makes no automated provider request: actual request-referrer and cache-header evidence remains pending a sanitized human browser journey. The artifact upload runs even when origin capture fails so a sanitized failure record is retained.

Authoritative policy sources reviewed on 2026-10-20:

- [OSM Foundation Tile Usage Policy](https://operations.osmfoundation.org/policies/tiles/)
- [OSM Foundation Attribution Guidelines](https://osmfoundation.org/wiki/Attribution_Guidelines)
- [OSM Foundation Produced Work Guideline](https://osmfoundation.org/wiki/Licence/Community_Guidelines/Produced_Work_-_Guideline)
- [Open Data Commons ODbL 1.0](https://opendatacommons.org/licenses/odbl/1-0/)

## Development verification

No Kubernetes command or live provider capture was run on this development machine.

- `npm run test:provider`: 8/8 passed, including allowlisted origin output, fail-closed edge behavior, and proof that capture makes no automated tile request.
- Deployment and release regressions: 11/11 passed.
- GitHub Actions YAML and the provider, cost, and story JSON files parsed successfully.
- The first unconfigured full-gate run stopped at the mandatory connected vertical slice because `DATABASE_URL` was absent; it was not treated as evidence. After `pg_isready` confirmed the repository's documented disposable PostGIS instance, the final owner run of `DATABASE_URL=postgres://postgres@127.0.0.1:55432/postgres npm run factory:verify` passed in 66.8 seconds with connected PostgreSQL cases executed.

## External completion evidence still required

Before TS-509 can truthfully become `done`, attach all of the following to the protected release record:

- a passing origin section in `release/provider-production-capture.json` from the exact deployed revision;
- sanitized request-referrer and cache-header evidence from a real human browser journey plus measured, privacy-safe production traffic;
- a public operator contact channel and accountable decision that actual use is suitable for the tile policy;
- accountable legal approval of attribution, export, derived-database, publication, and distribution decisions;
- selected hosting/provider quotes replacing planning allowances, agreed pilot/growth levels, and signed engineering/platform/finance budget decisions; and
- independent QA approval of those protected artifacts.

Until then, the checked-in review and release checklist remain blocked.

## Independent QA review

Independent QA reviewed the final worktree on 2026-10-20. It identified and prompted corrections for INC-075 through INC-077: an automated OSM tile probe that was not clearly permitted, a licensing gate that overlooked ODbL 4.6 public-use obligations and the GPX file's missing licence URI, and a CSP parser that could misread cumulatively enforced policies. The final implementation makes no provider request, blocks public route display/export pending accountable review and any required offer, records GPX notice adequacy as pending, and rejects multiple CSP policies rather than claiming a false allow result.

QA independently passed the provider contract 8/8, deployment/release contracts 11/11, `git diff --check`, and the database-backed `npm run factory:verify` in 66.8 seconds. QA approves source readiness only. Criteria 1, 2, 4, and 5 remain partial because protected deployment evidence and accountable approvals are absent; criterion 3 passes. The story must remain `in_review` with result `not_approved_missing_protected_evidence`.
