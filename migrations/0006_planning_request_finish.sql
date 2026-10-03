-- Historical requests predate this input and therefore remain explicitly unknown.
-- All new API submissions persist an observed boolean value.
ALTER TABLE planning_requests
    ADD COLUMN return_to_start boolean;

COMMENT ON COLUMN planning_requests.return_to_start IS
    'Immutable requested finish preference; NULL only for requests created before migration 0006.';
