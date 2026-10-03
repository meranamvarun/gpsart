ALTER TABLE generation_jobs
    ADD COLUMN status_version bigint NOT NULL DEFAULT 1 CHECK (status_version > 0),
    ADD CONSTRAINT generation_jobs_stage_known CHECK (
        stage IN ('queued', 'checking_roads', 'fitting_design', 'comparing_routes')
    );

-- Earlier rows did not require a public classification. Preserve ambiguous history as
-- explicitly unknown rather than inferring a reason that was never recorded.
UPDATE generation_job_outcomes
SET public_code = CASE
    WHEN outcome IN ('succeeded', 'cancelled', 'timed_out') THEN outcome::text
    ELSE 'legacy_unknown'
END
WHERE public_code IS NULL;

ALTER TABLE generation_job_outcomes
    ALTER COLUMN public_code SET NOT NULL,
    ADD CONSTRAINT generation_job_outcome_public_code_known CHECK (
        public_code IN (
            'succeeded',
            'unsupported_coverage',
            'no_routable_access',
            'disconnected_network',
            'insufficient_visual_match',
            'outside_distance',
            'exhausted_budget',
            'provider_unavailable',
            'attempts_exhausted',
            'cancelled',
            'timed_out',
            'legacy_unknown'
        )
    ),
    ADD CONSTRAINT generation_job_outcome_public_code_matches_state CHECK (
        (outcome = 'succeeded' AND public_code = 'succeeded') OR
        (outcome = 'no_match' AND public_code IN (
            'unsupported_coverage',
            'no_routable_access',
            'disconnected_network',
            'insufficient_visual_match',
            'outside_distance',
            'exhausted_budget',
            'legacy_unknown'
        )) OR
        (outcome = 'failed' AND public_code IN (
            'provider_unavailable',
            'attempts_exhausted',
            'legacy_unknown'
        )) OR
        (outcome = 'cancelled' AND public_code = 'cancelled') OR
        (outcome = 'timed_out' AND public_code = 'timed_out')
    ),
    ADD CONSTRAINT generation_job_outcome_budget_consistent CHECK (
        (NOT budget_exhausted OR public_code IN ('succeeded', 'exhausted_budget', 'timed_out')) AND
        (public_code NOT IN ('exhausted_budget', 'timed_out') OR budget_exhausted)
    );
