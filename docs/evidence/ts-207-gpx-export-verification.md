# TS-207 GPX export verification

The acceptance criteria are executable as follows:

1. `owned_route_exports_exact_gpx_with_safe_download_headers` rejects an unauthenticated request, authenticates an owner, calls `/api/v1/routes/{id}/export.gpx`, and compares every response byte with `tests/fixtures/route-export.gpx`. A second guest receives `404` for the same route.
2. The golden fixture and browser tests reject `time`, `speed`, `hr`, and `extensions` elements and require explicit planned-route metadata.
3. API assertions lock `application/gpx+xml; charset=utf-8`, `nosniff`, private no-store caching, and the predictable attachment filename. Rust and TypeScript hostile-name tests cover escaping and sanitization.
4. The external `gpx` 0.10 importer parses the response as GPX 1.1 and reproduces all five ordered coordinates exactly, including the self-intersection and closing/repeated point. Property-style canonical-coordinate cases round-trip boundary and sub-microdegree values without exponent notation. The serialization tolerance is therefore zero for stored decimal coordinates.
5. The Rust API and offline TypeScript serializer independently compare against the same golden file, establishing byte-level—and therefore geometric—equivalence for one immutable snapshot.
6. The checked external importer is `gpx` 0.10.0. It opens the golden pilot-style fixture in the automated API test. Compatibility is claimed only for GPX 1.1 conformance and that checked importer; no third-party service/account compatibility is claimed.

The GPX namespace and schema location use the immutable Topografix GPX 1.1 contract. Independent QA validated the golden file with .NET `XmlSchemaSet` against the official Topografix GPX 1.1 XSD on 2026-10-05. The automated `gpx` importer check is separate and runs without network access.

Independent QA also verified that `tests/fixtures/route-export.gpx` is checked out as LF on every platform via its scoped `.gitattributes` rule.
