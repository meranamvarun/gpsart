-- Existing candidate rows predate the complete immutable snapshot contract. Keep them
-- explicit as unavailable rather than fabricating missing classifications or metadata.
CREATE TABLE route_candidate_snapshots (
    candidate_id uuid PRIMARY KEY REFERENCES route_candidates(id) ON DELETE CASCADE,
    snapshot_json jsonb NOT NULL,
    CONSTRAINT route_candidate_snapshot_object CHECK (jsonb_typeof(snapshot_json) = 'object'),
    CONSTRAINT route_candidate_snapshot_identity CHECK (
        snapshot_json->>'id' = candidate_id::text
        AND snapshot_json->>'candidate_version' = snapshot_json#>>'{versions,candidate}'
        AND snapshot_json#>>'{versions,snapshot}' = 'route-candidate-snapshot-v1'
    ),
    CONSTRAINT route_candidate_snapshot_geojson CHECK (
        snapshot_json#>>'{geometry,type}' = 'LineString'
        AND jsonb_typeof(snapshot_json#>'{geometry,coordinates}') = 'array'
    ),
    CONSTRAINT route_candidate_snapshot_derived_views CHECK (
        (snapshot_json->>'duration_seconds')::bigint > 0
        AND jsonb_typeof(snapshot_json->'instructions') = 'array'
        AND jsonb_array_length(snapshot_json->'instructions') >= 2
        AND jsonb_typeof(snapshot_json->'traversals') = 'array'
        AND jsonb_array_length(snapshot_json->'traversals') >= 1
    ),
    CONSTRAINT route_candidate_snapshot_repeated_included CHECK (
        snapshot_json#>>'{distances,repeated_included_in_total}' = 'true'
    ),
    CONSTRAINT route_candidate_snapshot_legality_verified CHECK (
        snapshot_json#>>'{qualifications,legality_version}' = 'candidate-legality-v1'
        AND snapshot_json#>>'{qualifications,route_use_advisory_version}' = 'route-use-advisory-v1'
        AND snapshot_json#>>'{qualifications,access_verified}' = 'true'
        AND snapshot_json#>>'{qualifications,direction_verified}' = 'true'
        AND snapshot_json#>>'{qualifications,turns_verified}' = 'true'
    )
);

CREATE TRIGGER route_candidate_snapshots_are_immutable
BEFORE UPDATE ON route_candidate_snapshots
FOR EACH ROW EXECUTE FUNCTION reject_immutable_update();
