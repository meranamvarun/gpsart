ALTER TABLE generation_jobs
    ADD COLUMN max_attempts integer NOT NULL DEFAULT 3 CHECK (max_attempts > 0);

ALTER TABLE generation_jobs
    ADD CONSTRAINT generation_jobs_one_per_request UNIQUE (planning_request_id);

ALTER TABLE generation_job_outcomes
    ADD COLUMN budget_exhausted boolean NOT NULL DEFAULT false;

CREATE INDEX generation_jobs_claim_idx
    ON generation_jobs (queued_at, deadline_at)
    WHERE state IN ('queued', 'running');
