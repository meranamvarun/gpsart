CREATE EXTENSION IF NOT EXISTS postgis;

CREATE OR REPLACE FUNCTION reject_immutable_update()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION '% rows are immutable', TG_TABLE_NAME USING ERRCODE = '55000';
END;
$$;

CREATE TABLE guest_sessions (
    id uuid PRIMARY KEY,
    token_hash bytea NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    cleanup_after timestamptz NOT NULL,
    CONSTRAINT guest_session_expiry_order CHECK (expires_at > created_at),
    CONSTRAINT guest_session_cleanup_order CHECK (cleanup_after >= expires_at),
    UNIQUE (id, expires_at)
);

CREATE TABLE resolved_starts (
    id uuid PRIMARY KEY,
    guest_session_id uuid NOT NULL REFERENCES guest_sessions(id) ON DELETE CASCADE,
    requested_pin geometry(Point, 4326) NOT NULL,
    confirmed_start geometry(Point, 4326) NOT NULL,
    profile_version text NOT NULL,
    coverage_policy_version text NOT NULL,
    map_data_version text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    CONSTRAINT resolved_start_expiry_order CHECK (expires_at > created_at),
    CONSTRAINT resolved_start_requested_pin_valid CHECK (ST_IsValid(requested_pin)),
    CONSTRAINT resolved_start_confirmed_valid CHECK (ST_IsValid(confirmed_start)),
    UNIQUE (id, guest_session_id)
);

CREATE INDEX resolved_starts_requested_pin_gix ON resolved_starts USING gist (requested_pin);
CREATE INDEX resolved_starts_confirmed_start_gix ON resolved_starts USING gist (confirmed_start);

CREATE TABLE planning_requests (
    id uuid PRIMARY KEY,
    guest_session_id uuid NOT NULL REFERENCES guest_sessions(id) ON DELETE CASCADE,
    resolved_start_id uuid NOT NULL,
    revision integer NOT NULL CHECK (revision > 0),
    idempotency_key text NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 200),
    input_hash bytea NOT NULL CHECK (octet_length(input_hash) = 32),
    requested_pin geometry(Point, 4326) NOT NULL,
    requested_distance_metres integer NOT NULL CHECK (requested_distance_metres > 0),
    distance_tolerance_metres integer NOT NULL CHECK (distance_tolerance_metres >= 0),
    activity_profile jsonb NOT NULL CHECK (jsonb_typeof(activity_profile) = 'object'),
    activity_profile_version text NOT NULL,
    template_id text NOT NULL,
    template_version text NOT NULL,
    map_data_version text NOT NULL,
    algorithm_version text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    CONSTRAINT planning_request_expiry_order CHECK (expires_at > created_at),
    CONSTRAINT planning_request_start_owner FOREIGN KEY (resolved_start_id, guest_session_id)
        REFERENCES resolved_starts(id, guest_session_id),
    UNIQUE (guest_session_id, idempotency_key),
    UNIQUE (id, guest_session_id)
);

CREATE INDEX planning_requests_requested_pin_gix ON planning_requests USING gist (requested_pin);

CREATE TRIGGER planning_requests_are_immutable
BEFORE UPDATE ON planning_requests
FOR EACH ROW EXECUTE FUNCTION reject_immutable_update();

