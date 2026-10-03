use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, SystemTime},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartOwnership {
    pub id: Uuid,
    pub owner: Uuid,
    pub expires_at: SystemTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobInput {
    pub resolved_start_id: Uuid,
    pub revision: u32,
    pub idempotency_key: String,
    pub input_hash: [u8; 32],
    pub deadline_after: Duration,
    pub max_attempts: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Cancelled,
    TimedOut,
}

impl JobState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Cancelled | Self::TimedOut)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobLease {
    pub job_id: Uuid,
    pub worker_id: Uuid,
    pub attempt: u32,
    pub lease_expires_at: SystemTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Completion {
    Succeeded {
        candidate_ids: Vec<Uuid>,
        budget_exhausted: bool,
    },
    TimedOut,
}

#[derive(Clone)]
struct JobRecord {
    id: Uuid,
    owner: Uuid,
    input: JobInput,
    state: JobState,
    deadline_at: SystemTime,
    attempt: u32,
    lease: Option<JobLease>,
    candidate_ids: Vec<Uuid>,
    budget_exhausted: bool,
}

#[derive(Default)]
struct State {
    starts: HashMap<Uuid, StartOwnership>,
    jobs: HashMap<Uuid, JobRecord>,
    idempotency: HashMap<(Uuid, String), Uuid>,
}

#[derive(Default)]
pub struct InMemoryJobCoordinator {
    state: Mutex<State>,
}

impl InMemoryJobCoordinator {
    pub fn register_start(&self, start: StartOwnership) {
        self.state
            .lock()
            .expect("job lock")
            .starts
            .insert(start.id, start);
    }

    pub fn submit(&self, owner: Uuid, input: JobInput, now: SystemTime) -> Result<Uuid, JobError> {
        if input.idempotency_key.is_empty()
            || input.idempotency_key.len() > 200
            || input.revision == 0
            || input.deadline_after.is_zero()
            || input.max_attempts == 0
        {
            return Err(JobError::InvalidInput);
        }
        let mut state = self.state.lock().expect("job lock");
        let start = state.starts.get(&input.resolved_start_id);
        if !matches!(start, Some(start) if start.owner == owner && start.expires_at > now) {
            return Err(JobError::StartNotFound);
        }
        let key = (owner, input.idempotency_key.clone());
        if let Some(existing_id) = state.idempotency.get(&key) {
            let existing = state.jobs.get(existing_id).expect("idempotency target");
            return if existing.input.input_hash == input.input_hash {
                Ok(*existing_id)
            } else {
                Err(JobError::IdempotencyConflict)
            };
        }
        let id = Uuid::new_v4();
        let deadline_at = now + input.deadline_after;
        state.jobs.insert(
            id,
            JobRecord {
                id,
                owner,
                input: input.clone(),
                state: JobState::Queued,
                deadline_at,
                attempt: 0,
                lease: None,
                candidate_ids: Vec::new(),
                budget_exhausted: false,
            },
        );
        state.idempotency.insert(key, id);
        Ok(id)
    }

    pub fn claim(&self, worker_id: Uuid, now: SystemTime, lease_for: Duration) -> Option<JobLease> {
        let mut state = self.state.lock().expect("job lock");
        let job = state
            .jobs
            .values_mut()
            .filter(|job| {
                !job.state.is_terminal()
                    && job.deadline_at > now
                    && job.attempt < job.input.max_attempts
                    && job
                        .lease
                        .as_ref()
                        .is_none_or(|lease| lease.lease_expires_at <= now)
            })
            .min_by_key(|job| job.id)?;
        job.attempt += 1;
        job.state = JobState::Running;
        let lease = JobLease {
            job_id: job.id,
            worker_id,
            attempt: job.attempt,
            lease_expires_at: now + lease_for,
        };
        job.lease = Some(lease.clone());
        Some(lease)
    }

    pub fn renew(
        &self,
        lease: &JobLease,
        now: SystemTime,
        lease_for: Duration,
    ) -> Result<JobLease, JobError> {
        let mut state = self.state.lock().expect("job lock");
        let job = state
            .jobs
            .get_mut(&lease.job_id)
            .ok_or(JobError::NotFound)?;
        require_lease(job, lease, now)?;
        let renewed = JobLease {
            lease_expires_at: now + lease_for,
            ..lease.clone()
        };
        job.lease = Some(renewed.clone());
        Ok(renewed)
    }

    pub fn cancel(
        &self,
        owner: Uuid,
        job_id: Uuid,
        _now: SystemTime,
    ) -> Result<JobState, JobError> {
        let mut state = self.state.lock().expect("job lock");
        let job = state
            .jobs
            .get_mut(&job_id)
            .filter(|job| job.owner == owner)
            .ok_or(JobError::NotFound)?;
        if job.state.is_terminal() {
            return Ok(job.state);
        }
        job.state = JobState::Cancelled;
        job.lease = None;
        Ok(job.state)
    }

    pub fn may_schedule_provider(
        &self,
        lease: &JobLease,
        now: SystemTime,
        required_budget: Duration,
    ) -> bool {
        let state = self.state.lock().expect("job lock");
        state.jobs.get(&lease.job_id).is_some_and(|job| {
            require_lease(job, lease, now).is_ok()
                && job
                    .deadline_at
                    .duration_since(now)
                    .is_ok_and(|left| left >= required_budget)
        })
    }

    pub fn complete(
        &self,
        lease: JobLease,
        completion: Completion,
        now: SystemTime,
    ) -> Result<(), JobError> {
        let mut state = self.state.lock().expect("job lock");
        let job = state
            .jobs
            .get_mut(&lease.job_id)
            .ok_or(JobError::NotFound)?;
        if job.state.is_terminal() {
            return Err(JobError::Terminal);
        }
        require_lease(job, &lease, now)?;
        match completion {
            Completion::Succeeded {
                candidate_ids,
                budget_exhausted,
            } if !candidate_ids.is_empty() => {
                job.state = JobState::Succeeded;
                job.candidate_ids = candidate_ids;
                job.budget_exhausted = budget_exhausted || now >= job.deadline_at;
            }
            Completion::Succeeded { .. } | Completion::TimedOut => job.state = JobState::TimedOut,
        }
        job.lease = None;
        Ok(())
    }

    pub fn state(&self, job_id: Uuid) -> Option<JobState> {
        self.state
            .lock()
            .expect("job lock")
            .jobs
            .get(&job_id)
            .map(|job| job.state)
    }
}

fn require_lease(job: &JobRecord, lease: &JobLease, now: SystemTime) -> Result<(), JobError> {
    if job.lease.as_ref() == Some(lease) && lease.lease_expires_at > now {
        Ok(())
    } else {
        Err(JobError::LeaseLost)
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum JobError {
    #[error("route-job input is invalid")]
    InvalidInput,
    #[error("resolved start was not found")]
    StartNotFound,
    #[error("idempotency key was reused with different input")]
    IdempotencyConflict,
    #[error("job was not found")]
    NotFound,
    #[error("worker lease was lost")]
    LeaseLost,
    #[error("job is already terminal")]
    Terminal,
}
