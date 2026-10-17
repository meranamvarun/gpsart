# Production routing data update

The approved pilot input is `ops/routing/production-pilot-v1.json`. Treat its configuration version, coverage version, graph snapshot ID, checksum-derived map-data version, and reviewed starts as immutable.

To publish a successor:

1. Capture the intended OpenStreetMap API response once. Record its HTTPS bounding-box URL, UTC retrieval time, and SHA-256; retain the source bytes in the controlled release evidence store rather than committing contributor or user data.
2. Run the exact extraction command recorded in the proposed manifest. Verify the extractor script checksum before running it. Never edit generated graph bytes or their checksum by hand.
3. Assign new append-only snapshot, map-data, coverage, and configuration versions. Never replace a graph under an existing version.
4. Review each proposed start against that exact graph for each explicitly listed activity profile. Omit an activity when access or direction is unknown. Do not snap, widen tolerance, or infer a profile from another profile's review.
5. Re-run the boundary, outside-coverage, isolated-start, direction-restriction, unknown-access, checksum, empty-input, malformed-input, and version-mismatch tests. Record the reviewer, review date, and signed decision in the new manifest.
6. Run `cargo test -p gpsart-config --test production_routing`, the routing suite, and `npm run factory:verify` before deployment.
7. Keep every previous manifest and graph addressable. Stored requests and candidates retain their original `map_data_version`; the compatibility test loads the old v1 manifest after any successor is added, proving those snapshots remain interpretable.

Rollback selects a previously approved immutable manifest and its exact graph bytes. It must never relabel new bytes with an old version.
