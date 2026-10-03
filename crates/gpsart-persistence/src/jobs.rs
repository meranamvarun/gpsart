use sqlx::{PgPool, Row};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, SystemTime},
};
use thiserror::Error;
use time::OffsetDateTime;
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
    #[error("job storage is unavailable")]
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct RouteJobSubmission {
    pub resolved_start_id: Uuid,
    pub revision: i32,
    pub idempotency_key: String,
    pub input_hash: [u8; 32],
    pub requested_distance_metres: i32,
    pub distance_tolerance_metres: i32,
    pub activity_profile_json: String,
    pub activity_profile_version: String,
    pub template_id: String,
    pub template_version: String,
    pub map_data_version: String,
    pub algorithm_version: String,
    pub expires_at: SystemTime,
    pub deadline_at: SystemTime,
    pub max_attempts: i32,
}

#[derive(Clone)]
pub struct PostgresJobCoordinator {
    pool: PgPool,
}

impl PostgresJobCoordinator {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn submit(&self, owner: Uuid, input: &RouteJobSubmission) -> Result<Uuid, JobError> {
        if input.revision <= 0
            || input.idempotency_key.is_empty()
            || input.idempotency_key.len() > 200
            || input.requested_distance_metres <= 0
            || input.distance_tolerance_metres < 0
            || input.max_attempts <= 0
            || input.deadline_at <= SystemTime::now()
        {
            return Err(JobError::InvalidInput);
        }
        let mut tx = self.pool.begin().await.map_err(|_| JobError::Unavailable)?;
        if let Some(row) = sqlx::query("SELECT pr.input_hash, gj.id FROM planning_requests pr JOIN generation_jobs gj ON gj.planning_request_id = pr.id WHERE pr.guest_session_id = $1 AND pr.idempotency_key = $2 FOR UPDATE")
            .bind(owner).bind(&input.idempotency_key).fetch_optional(&mut *tx).await.map_err(|_| JobError::Unavailable)? {
            let hash: Vec<u8> = row.try_get("input_hash").map_err(|_| JobError::Unavailable)?;
            let id: Uuid = row.try_get("id").map_err(|_| JobError::Unavailable)?;
            return if hash == input.input_hash { Ok(id) } else { Err(JobError::IdempotencyConflict) };
        }
        let request_id = Uuid::new_v4();
        let inserted = sqlx::query("INSERT INTO planning_requests (id, guest_session_id, resolved_start_id, revision, idempotency_key, input_hash, requested_pin, requested_distance_metres, distance_tolerance_metres, activity_profile, activity_profile_version, template_id, template_version, map_data_version, algorithm_version, expires_at) SELECT $1, $2, rs.id, $3, $4, $5, rs.confirmed_start, $6, $7, $8::jsonb, $9, $10, $11, $12, $13, $14 FROM resolved_starts rs WHERE rs.id = $15 AND rs.guest_session_id = $2 AND rs.expires_at > now() AND rs.profile_version = $9 AND rs.map_data_version = $12 RETURNING id")
            .bind(request_id).bind(owner).bind(input.revision).bind(&input.idempotency_key)
            .bind(input.input_hash.as_slice()).bind(input.requested_distance_metres).bind(input.distance_tolerance_metres)
            .bind(&input.activity_profile_json).bind(&input.activity_profile_version).bind(&input.template_id)
            .bind(&input.template_version).bind(&input.map_data_version).bind(&input.algorithm_version)
            .bind(OffsetDateTime::from(input.expires_at)).bind(input.resolved_start_id)
            .fetch_optional(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        if inserted.is_none() {
            return Err(JobError::StartNotFound);
        }
        let job_id = Uuid::new_v4();
        sqlx::query("INSERT INTO generation_jobs (id, guest_session_id, planning_request_id, deadline_at, max_attempts) VALUES ($1,$2,$3,$4,$5)")
            .bind(job_id).bind(owner).bind(request_id).bind(OffsetDateTime::from(input.deadline_at)).bind(input.max_attempts)
            .execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        tx.commit().await.map_err(|_| JobError::Unavailable)?;
        Ok(job_id)
    }

    pub async fn claim(
        &self,
        worker: Uuid,
        now: SystemTime,
        lease_for: Duration,
    ) -> Result<Option<JobLease>, JobError> {
        let mut tx = self.pool.begin().await.map_err(|_| JobError::Unavailable)?;
        let row = sqlx::query("WITH claimable AS (SELECT id FROM generation_jobs WHERE state IN ('queued','running') AND deadline_at > $1 AND attempt_count < max_attempts AND (lease_expires_at IS NULL OR lease_expires_at <= $1) ORDER BY queued_at FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE generation_jobs j SET state='running', started_at=COALESCE(started_at,$1), lease_owner=$2, lease_expires_at=$3, attempt_count=attempt_count+1 FROM claimable WHERE j.id=claimable.id RETURNING j.id,j.attempt_count,j.lease_expires_at")
            .bind(OffsetDateTime::from(now)).bind(worker).bind(OffsetDateTime::from(now + lease_for))
            .fetch_optional(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        let Some(row) = row else {
            tx.commit().await.map_err(|_| JobError::Unavailable)?;
            return Ok(None);
        };
        let job_id: Uuid = row.try_get("id").map_err(|_| JobError::Unavailable)?;
        let attempt: i32 = row
            .try_get("attempt_count")
            .map_err(|_| JobError::Unavailable)?;
        let expires: OffsetDateTime = row
            .try_get("lease_expires_at")
            .map_err(|_| JobError::Unavailable)?;
        sqlx::query("INSERT INTO job_attempts (id,generation_job_id,attempt_number,worker_id) VALUES ($1,$2,$3,$4)")
            .bind(Uuid::new_v4()).bind(job_id).bind(attempt).bind(worker).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        tx.commit().await.map_err(|_| JobError::Unavailable)?;
        Ok(Some(JobLease {
            job_id,
            worker_id: worker,
            attempt: attempt as u32,
            lease_expires_at: SystemTime::from(expires),
        }))
    }

    pub async fn renew(
        &self,
        lease: &JobLease,
        now: SystemTime,
        lease_for: Duration,
    ) -> Result<JobLease, JobError> {
        let expires = now + lease_for;
        let affected = sqlx::query("UPDATE generation_jobs SET lease_expires_at=$1 WHERE id=$2 AND state='running' AND lease_owner=$3 AND attempt_count=$4 AND lease_expires_at>$5 AND deadline_at>$5")
            .bind(OffsetDateTime::from(expires)).bind(lease.job_id).bind(lease.worker_id).bind(lease.attempt as i32).bind(OffsetDateTime::from(now))
            .execute(&self.pool).await.map_err(|_| JobError::Unavailable)?.rows_affected();
        if affected == 1 {
            Ok(JobLease {
                lease_expires_at: expires,
                ..lease.clone()
            })
        } else {
            Err(JobError::LeaseLost)
        }
    }

    pub async fn may_schedule_provider(
        &self,
        lease: &JobLease,
        now: SystemTime,
        required: Duration,
    ) -> Result<bool, JobError> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE id=$1 AND state='running' AND cancellation_requested_at IS NULL AND lease_owner=$2 AND attempt_count=$3 AND lease_expires_at>$4 AND deadline_at>=$5)")
            .bind(lease.job_id).bind(lease.worker_id).bind(lease.attempt as i32).bind(OffsetDateTime::from(now)).bind(OffsetDateTime::from(now + required))
            .fetch_one(&self.pool).await.map_err(|_| JobError::Unavailable)
    }

    pub async fn cancel(
        &self,
        owner: Uuid,
        job_id: Uuid,
        now: SystemTime,
    ) -> Result<JobState, JobError> {
        let mut tx = self.pool.begin().await.map_err(|_| JobError::Unavailable)?;
        let row = sqlx::query("SELECT state::text AS state FROM generation_jobs WHERE id=$1 AND guest_session_id=$2 FOR UPDATE")
            .bind(job_id).bind(owner).fetch_optional(&mut *tx).await.map_err(|_| JobError::Unavailable)?.ok_or(JobError::NotFound)?;
        let state: String = row.try_get("state").map_err(|_| JobError::Unavailable)?;
        if matches!(
            state.as_str(),
            "succeeded" | "no_match" | "failed" | "cancelled" | "timed_out"
        ) {
            tx.commit().await.map_err(|_| JobError::Unavailable)?;
            return Ok(parse_state(&state));
        }
        sqlx::query("UPDATE generation_jobs SET state='cancelled', cancellation_requested_at=$1, completed_at=$1, lease_owner=NULL, lease_expires_at=NULL WHERE id=$2")
            .bind(OffsetDateTime::from(now)).bind(job_id).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        sqlx::query("INSERT INTO generation_job_outcomes (generation_job_id,outcome,public_code) VALUES ($1,'cancelled','cancelled')")
            .bind(job_id).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        tx.commit().await.map_err(|_| JobError::Unavailable)?;
        Ok(JobState::Cancelled)
    }

    pub async fn complete(
        &self,
        lease: &JobLease,
        completion: &Completion,
        now: SystemTime,
    ) -> Result<JobState, JobError> {
        let mut tx = self.pool.begin().await.map_err(|_| JobError::Unavailable)?;
        let row = sqlx::query(
            "SELECT state::text AS state, deadline_at FROM generation_jobs WHERE id=$1 FOR UPDATE",
        )
        .bind(lease.job_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| JobError::Unavailable)?
        .ok_or(JobError::NotFound)?;
        let state: String = row.try_get("state").map_err(|_| JobError::Unavailable)?;
        if matches!(
            state.as_str(),
            "succeeded" | "no_match" | "failed" | "cancelled" | "timed_out"
        ) {
            return Err(JobError::Terminal);
        }
        let valid_lease: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE id=$1 AND state='running' AND lease_owner=$2 AND attempt_count=$3 AND lease_expires_at>$4)")
            .bind(lease.job_id).bind(lease.worker_id).bind(lease.attempt as i32).bind(OffsetDateTime::from(now))
            .fetch_one(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        if !valid_lease {
            return Err(JobError::LeaseLost);
        }
        let (next, budget_exhausted) = match completion {
            Completion::Succeeded {
                candidate_ids,
                budget_exhausted,
            } if !candidate_ids.is_empty() => {
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM route_candidates WHERE generation_job_id=$1 AND id=ANY($2)")
                    .bind(lease.job_id).bind(candidate_ids).fetch_one(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
                if count == candidate_ids.len() as i64 {
                    (JobState::Succeeded, *budget_exhausted)
                } else {
                    (JobState::TimedOut, true)
                }
            }
            Completion::Succeeded { .. } | Completion::TimedOut => (JobState::TimedOut, true),
        };
        let state_name = match next {
            JobState::Succeeded => "succeeded",
            _ => "timed_out",
        };
        sqlx::query("UPDATE generation_jobs SET state=$1::generation_job_state, completed_at=$2, lease_owner=NULL, lease_expires_at=NULL WHERE id=$3")
            .bind(state_name).bind(OffsetDateTime::from(now)).bind(lease.job_id).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        sqlx::query("UPDATE job_attempts SET finished_at=$1,result_code=$2 WHERE generation_job_id=$3 AND attempt_number=$4")
            .bind(OffsetDateTime::from(now)).bind(state_name).bind(lease.job_id).bind(lease.attempt as i32).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        sqlx::query("INSERT INTO generation_job_outcomes (generation_job_id,outcome,public_code,budget_exhausted) VALUES ($1,$2::generation_terminal_outcome,$3,$4)")
            .bind(lease.job_id).bind(state_name).bind(state_name).bind(budget_exhausted || matches!(next, JobState::Succeeded) && now >= SystemTime::from(row.try_get::<OffsetDateTime,_>("deadline_at").map_err(|_| JobError::Unavailable)?))
            .execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        tx.commit().await.map_err(|_| JobError::Unavailable)?;
        Ok(next)
    }
}

fn parse_state(state: &str) -> JobState {
    match state {
        "running" => JobState::Running,
        "succeeded" => JobState::Succeeded,
        "cancelled" => JobState::Cancelled,
        "timed_out" => JobState::TimedOut,
        _ => JobState::Queued,
    }
}
