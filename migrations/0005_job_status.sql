ALTER TABLE generation_jobs
    ADD COLUMN status_version bigint NOT NULL DEFAULT 1 CHECK (status_version > 0),
    ADD CONSTRAINT generation_jobs_stage_known CHECK (
        stage IN ('queued', 'checking_roads', 'fitting_design', 'comparing_routes')
    );

ALTER TABLE generation_job_outcomes
    ADD COLUMN result_ids uuid[] NOT NULL DEFAULT '{}';

-- Earlier rows did not require a public classification. Preserve ambiguous history as
-- explicitly unknown rather than inferring a reason that was never recorded.
UPDATE generation_job_outcomes
SET public_code = CASE
    WHEN outcome IN ('succeeded', 'cancelled', 'timed_out') THEN outcome::text
    ELSE 'legacy_unknown'
END
WHERE public_code IS NULL;

-- Freeze the exact result set that an earlier successful completion made available.
-- Candidate rows remain independent immutable snapshots; later rows must not silently
-- alter the terminal job representation or its ETag.
UPDATE generation_job_outcomes outcome
SET result_ids = published.ids
FROM (
    SELECT generation_job_id, array_agg(id ORDER BY candidate_version) AS ids
    FROM route_candidates
    GROUP BY generation_job_id
) AS published
WHERE outcome.generation_job_id = published.generation_job_id
  AND outcome.outcome = 'succeeded';

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
    ),
    ADD CONSTRAINT generation_job_outcome_results_match_state CHECK (
        (outcome = 'succeeded' AND cardinality(result_ids) > 0) OR
        (outcome <> 'succeeded' AND cardinality(result_ids) = 0)
    );

CREATE TRIGGER generation_job_outcomes_are_immutable
BEFORE UPDATE ON generation_job_outcomes
FOR EACH ROW EXECUTE FUNCTION reject_immutable_update();
