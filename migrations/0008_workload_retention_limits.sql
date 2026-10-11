ALTER TABLE generation_jobs
    ADD COLUMN provider_call_count integer NOT NULL DEFAULT 0
        CHECK (provider_call_count >= 0);

ALTER TABLE cleanup_runs
    ADD COLUMN deleted_job_count integer NOT NULL DEFAULT 0 CHECK (deleted_job_count >= 0),
    ADD COLUMN deleted_candidate_count integer NOT NULL DEFAULT 0 CHECK (deleted_candidate_count >= 0),
    ADD COLUMN deleted_start_count integer NOT NULL DEFAULT 0 CHECK (deleted_start_count >= 0),
    ADD COLUMN retry_required boolean NOT NULL DEFAULT false;

CREATE INDEX planning_requests_guest_created_idx
    ON planning_requests (guest_session_id, created_at DESC);

CREATE INDEX planning_requests_expiry_idx ON planning_requests (expires_at);
CREATE INDEX route_candidates_expiry_idx ON route_candidates (expires_at);
CREATE INDEX resolved_starts_expiry_idx ON resolved_starts (expires_at);
