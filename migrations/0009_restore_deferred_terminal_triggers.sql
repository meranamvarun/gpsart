-- The supported upgrader temporarily makes these triggers immediate while a populated
-- v4 database crosses migration 0005. Restore the normal atomic state/outcome contract
-- for both repaired upgrades and clean installations.
DROP TRIGGER generation_jobs_terminal_outcome_check ON generation_jobs;
DROP TRIGGER generation_job_outcomes_state_check ON generation_job_outcomes;

CREATE CONSTRAINT TRIGGER generation_jobs_terminal_outcome_check
AFTER INSERT OR UPDATE OF state OR DELETE ON generation_jobs
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION enforce_one_terminal_outcome();

CREATE CONSTRAINT TRIGGER generation_job_outcomes_state_check
AFTER INSERT OR UPDATE OR DELETE ON generation_job_outcomes
DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW EXECUTE FUNCTION enforce_one_terminal_outcome();
