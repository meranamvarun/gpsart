CREATE TYPE generation_job_state AS ENUM (
    'queued', 'running', 'succeeded', 'no_match', 'failed', 'cancelled', 'timed_out'
);

CREATE TYPE generation_terminal_outcome AS ENUM (
    'succeeded', 'no_match', 'failed', 'cancelled', 'timed_out'
);

CREATE TABLE generation_jobs (
    id uuid PRIMARY KEY,
    guest_session_id uuid NOT NULL REFERENCES guest_sessions(id) ON DELETE CASCADE,
    planning_request_id uuid NOT NULL,
    state generation_job_state NOT NULL DEFAULT 'queued',
    stage text NOT NULL DEFAULT 'queued',
    queued_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    deadline_at timestamptz NOT NULL,
    completed_at timestamptz,
    lease_owner uuid,
    lease_expires_at timestamptz,
    cancellation_requested_at timestamptz,
    attempt_count integer NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    CONSTRAINT generation_job_request_owner FOREIGN KEY (planning_request_id, guest_session_id)
        REFERENCES planning_requests(id, guest_session_id),
    CONSTRAINT generation_job_deadline_order CHECK (deadline_at > queued_at),
    CONSTRAINT generation_job_start_order CHECK (started_at IS NULL OR started_at >= queued_at),
    CONSTRAINT generation_job_completion_matches_state CHECK (
        (state IN ('queued', 'running') AND completed_at IS NULL)
        OR (state IN ('succeeded', 'no_match', 'failed', 'cancelled', 'timed_out') AND completed_at IS NOT NULL)
    ),
    CONSTRAINT generation_job_lease_pair CHECK (
        (lease_owner IS NULL AND lease_expires_at IS NULL)
        OR (lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL)
    ),
    UNIQUE (id, guest_session_id)
);

CREATE INDEX generation_jobs_queue_idx
    ON generation_jobs (state, queued_at)
    WHERE state IN ('queued', 'running');

CREATE TABLE generation_job_outcomes (
    generation_job_id uuid PRIMARY KEY REFERENCES generation_jobs(id) ON DELETE CASCADE,
    outcome generation_terminal_outcome NOT NULL,
    public_code text,
    recorded_at timestamptz NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION enforce_one_terminal_outcome()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    checked_job_id uuid := COALESCE(
        (to_jsonb(NEW)->>'generation_job_id')::uuid,
        (to_jsonb(NEW)->>'id')::uuid,
        (to_jsonb(OLD)->>'generation_job_id')::uuid,
        (to_jsonb(OLD)->>'id')::uuid
    );
    current_state generation_job_state;
    outcome_count integer;
    recorded_outcome generation_terminal_outcome;
BEGIN
    SELECT state INTO current_state FROM generation_jobs WHERE id = checked_job_id;
    IF NOT FOUND THEN
        RETURN NULL;
    END IF;
    SELECT count(*), max(outcome::text)::generation_terminal_outcome INTO outcome_count, recorded_outcome
      FROM generation_job_outcomes WHERE generation_job_id = checked_job_id;
    IF current_state IN ('queued', 'running') AND outcome_count <> 0 THEN
        RAISE EXCEPTION 'active job cannot have a terminal outcome' USING ERRCODE = '23514';
    END IF;
    IF current_state NOT IN ('queued', 'running')
       AND (outcome_count <> 1 OR recorded_outcome::text <> current_state::text) THEN
        RAISE EXCEPTION 'terminal job must have exactly one matching outcome' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER generation_jobs_terminal_outcome_check
AFTER INSERT OR UPDATE OF state OR DELETE ON generation_jobs
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION enforce_one_terminal_outcome();

CREATE CONSTRAINT TRIGGER generation_job_outcomes_state_check
AFTER INSERT OR UPDATE OR DELETE ON generation_job_outcomes
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION enforce_one_terminal_outcome();

CREATE TABLE job_attempts (
    id uuid PRIMARY KEY,
    generation_job_id uuid NOT NULL REFERENCES generation_jobs(id) ON DELETE CASCADE,
    attempt_number integer NOT NULL CHECK (attempt_number > 0),
    worker_id uuid NOT NULL,
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    result_code text,
    CONSTRAINT job_attempt_finish_order CHECK (finished_at IS NULL OR finished_at >= started_at),
    UNIQUE (generation_job_id, attempt_number)
);

CREATE TABLE route_candidates (
    id uuid PRIMARY KEY,
    guest_session_id uuid NOT NULL REFERENCES guest_sessions(id) ON DELETE CASCADE,
    generation_job_id uuid NOT NULL,
    candidate_version integer NOT NULL CHECK (candidate_version > 0),
    start_point geometry(Point, 4326) NOT NULL,
    end_point geometry(Point, 4326) NOT NULL,
    canonical_route geometry(LineString, 4326) NOT NULL,
    distance_metres integer NOT NULL CHECK (distance_metres > 0),
    map_data_version text NOT NULL,
    algorithm_version text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    CONSTRAINT route_candidate_job_owner FOREIGN KEY (generation_job_id, guest_session_id)
        REFERENCES generation_jobs(id, guest_session_id),
    CONSTRAINT route_candidate_expiry_order CHECK (expires_at > created_at),
    CONSTRAINT route_candidate_geometry_valid CHECK (
        ST_IsValid(start_point) AND ST_IsValid(end_point) AND ST_IsValid(canonical_route)
    ),
    UNIQUE (generation_job_id, candidate_version),
    UNIQUE (id, candidate_version)
);

CREATE INDEX route_candidates_route_gix ON route_candidates USING gist (canonical_route);

CREATE TRIGGER route_candidates_are_immutable
BEFORE UPDATE ON route_candidates
FOR EACH ROW EXECUTE FUNCTION reject_immutable_update();

CREATE TABLE cleanup_runs (
    id uuid PRIMARY KEY,
    started_at timestamptz NOT NULL DEFAULT now(),
    finished_at timestamptz,
    cutoff_at timestamptz NOT NULL,
    status text NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    deleted_session_count integer NOT NULL DEFAULT 0 CHECK (deleted_session_count >= 0),
    retry_after timestamptz,
    CONSTRAINT cleanup_run_finish_order CHECK (finished_at IS NULL OR finished_at >= started_at)
);

CREATE TABLE cleanup_run_failures (
    cleanup_run_id uuid NOT NULL REFERENCES cleanup_runs(id) ON DELETE CASCADE,
    guest_session_id uuid NOT NULL,
    retry_count integer NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    public_code text NOT NULL,
    retry_after timestamptz NOT NULL,
    PRIMARY KEY (cleanup_run_id, guest_session_id)
);
