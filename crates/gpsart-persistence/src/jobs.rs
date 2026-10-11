use gpsart_config::WorkloadLimits;
use gpsart_observability::{Correlation, MetricEvent, MetricName};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgPoolOptions};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, SystemTime},
};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

pub const SUPPORTED_ALGORITHM_VERSION: &str = "algorithm-v1";

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
    NoMatch,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStage {
    Queued,
    CheckingRoads,
    FittingDesign,
    ComparingRoutes,
}

impl JobStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::CheckingRoads => "checking_roads",
            Self::FittingDesign => "fitting_design",
            Self::ComparingRoutes => "comparing_routes",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobOutcomeCode {
    Succeeded,
    UnsupportedCoverage,
    NoRoutableAccess,
    DisconnectedNetwork,
    InsufficientVisualMatch,
    OutsideDistance,
    ExhaustedBudget,
    ProviderUnavailable,
    AttemptsExhausted,
    Cancelled,
    TimedOut,
    LegacyUnknown,
}

impl JobOutcomeCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::UnsupportedCoverage => "unsupported_coverage",
            Self::NoRoutableAccess => "no_routable_access",
            Self::DisconnectedNetwork => "disconnected_network",
            Self::InsufficientVisualMatch => "insufficient_visual_match",
            Self::OutsideDistance => "outside_distance",
            Self::ExhaustedBudget => "exhausted_budget",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::AttemptsExhausted => "attempts_exhausted",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::LegacyUnknown => "legacy_unknown",
        }
    }

    pub const fn matches_state(self, state: JobState) -> bool {
        match state {
            JobState::Succeeded => matches!(self, Self::Succeeded),
            JobState::NoMatch => matches!(
                self,
                Self::UnsupportedCoverage
                    | Self::NoRoutableAccess
                    | Self::DisconnectedNetwork
                    | Self::InsufficientVisualMatch
                    | Self::OutsideDistance
                    | Self::ExhaustedBudget
                    | Self::LegacyUnknown
            ),
            JobState::Failed => matches!(
                self,
                Self::ProviderUnavailable | Self::AttemptsExhausted | Self::LegacyUnknown
            ),
            JobState::Cancelled => matches!(self, Self::Cancelled),
            JobState::TimedOut => matches!(self, Self::TimedOut),
            JobState::Queued | JobState::Running => false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobOutcomeSnapshot {
    pub code: JobOutcomeCode,
    pub budget_exhausted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobStatusSnapshot {
    pub job_id: Uuid,
    pub request_revision: i32,
    pub state: JobState,
    pub stage: JobStage,
    pub status_version: i64,
    pub queued_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub completed_at: Option<SystemTime>,
    pub deadline_at: SystemTime,
    pub expires_at: SystemTime,
    pub outcome: Option<JobOutcomeSnapshot>,
    pub result_ids: Vec<Uuid>,
}

impl JobState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::NoMatch | Self::Failed | Self::Cancelled | Self::TimedOut
        )
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
    NoMatch {
        code: JobOutcomeCode,
    },
    Failed {
        code: JobOutcomeCode,
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
        for job in state.jobs.values_mut().filter(|job| {
            !job.state.is_terminal()
                && job
                    .lease
                    .as_ref()
                    .is_none_or(|lease| lease.lease_expires_at <= now)
        }) {
            if job.deadline_at <= now {
                job.state = JobState::TimedOut;
                job.lease = None;
            } else if job.attempt >= job.input.max_attempts {
                job.state = JobState::Failed;
                job.lease = None;
            }
        }
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
            Completion::Succeeded { .. } | Completion::TimedOut if now >= job.deadline_at => {
                job.state = JobState::TimedOut
            }
            Completion::NoMatch { code }
                if code.matches_state(JobState::NoMatch)
                    && code != JobOutcomeCode::LegacyUnknown =>
            {
                job.state = JobState::NoMatch
            }
            Completion::Failed { code }
                if code.matches_state(JobState::Failed)
                    && code != JobOutcomeCode::LegacyUnknown =>
            {
                job.state = JobState::Failed
            }
            Completion::Succeeded { .. }
            | Completion::NoMatch { .. }
            | Completion::Failed { .. }
            | Completion::TimedOut => {
                return Err(JobError::InvalidInput);
            }
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
    #[error("route-job request exceeds the configured payload limit")]
    PayloadTooLarge,
    #[error("route-job admission limit reached; retry later")]
    RateLimited { retry_after_seconds: u64 },
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
    pub return_to_start: bool,
    pub activity_profile_json: String,
    pub activity_profile_version: String,
    pub coverage_policy_version: String,
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
    limits: WorkloadLimits,
}

pub struct PostgresProviderCallPermit {
    transaction: Option<Transaction<'static, Postgres>>,
}

impl PostgresProviderCallPermit {
    pub async fn commit(mut self) -> Result<(), JobError> {
        self.transaction
            .take()
            .expect("provider-call permit transaction")
            .commit()
            .await
            .map_err(|_| JobError::Unavailable)
    }
}

impl PostgresJobCoordinator {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            limits: WorkloadLimits::default(),
        }
    }

    pub fn with_limits(pool: PgPool, limits: WorkloadLimits) -> Self {
        Self { pool, limits }
    }

    pub async fn connect(database_url: &str) -> Result<Self, JobError> {
        Self::connect_with_limits(database_url, WorkloadLimits::default()).await
    }

    pub async fn connect_with_limits(
        database_url: &str,
        limits: WorkloadLimits,
    ) -> Result<Self, JobError> {
        PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map(|pool| Self::with_limits(pool, limits))
            .map_err(|_| JobError::Unavailable)
    }

    pub async fn submit(&self, owner: Uuid, input: &RouteJobSubmission) -> Result<Uuid, JobError> {
        let now = SystemTime::now();
        if input.revision <= 0
            || input.idempotency_key.is_empty()
            || input.idempotency_key.len() > 200
            || !(1_000..=self.limits.max_distance_metres).contains(&input.requested_distance_metres)
            || input.distance_tolerance_metres < 0
            || !self.limits.allowed_tolerance_percent.iter().any(|percent| {
                (i64::from(input.distance_tolerance_metres) * 100
                    - i64::from(input.requested_distance_metres) * i64::from(*percent))
                .abs()
                    <= 50
            })
            || input.max_attempts <= 0
            || input.max_attempts > self.limits.max_candidate_attempts
            || input.deadline_at <= now
            || input
                .deadline_at
                .duration_since(now)
                .map_or(true, |value| value > self.limits.max_runtime)
            || input.expires_at <= input.deadline_at
            || input
                .expires_at
                .duration_since(now)
                .map_or(true, |value| value > self.limits.guest_retention)
            || input.algorithm_version != SUPPORTED_ALGORITHM_VERSION
            || [
                &input.activity_profile_version,
                &input.coverage_policy_version,
                &input.template_id,
                &input.template_version,
                &input.map_data_version,
                &input.algorithm_version,
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(JobError::InvalidInput);
        }
        let mut tx = self.pool.begin().await.map_err(|_| JobError::Unavailable)?;
        sqlx::query("SELECT pg_advisory_xact_lock(401)")
            .execute(&mut *tx)
            .await
            .map_err(|_| JobError::Unavailable)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("{owner}:{}", input.idempotency_key))
            .execute(&mut *tx)
            .await
            .map_err(|_| JobError::Unavailable)?;
        if let Some(row) = sqlx::query("SELECT pr.input_hash, gj.id FROM planning_requests pr JOIN generation_jobs gj ON gj.planning_request_id = pr.id WHERE pr.guest_session_id = $1 AND pr.idempotency_key = $2 ORDER BY gj.queued_at LIMIT 1 FOR UPDATE OF pr, gj")
            .bind(owner).bind(&input.idempotency_key).fetch_optional(&mut *tx).await.map_err(|_| JobError::Unavailable)? {
            let hash: Vec<u8> = row.try_get("input_hash").map_err(|_| JobError::Unavailable)?;
            let id: Uuid = row.try_get("id").map_err(|_| JobError::Unavailable)?;
            return if hash == input.input_hash { Ok(id) } else { Err(JobError::IdempotencyConflict) };
        }
        let active_for_guest: i64 = sqlx::query_scalar("SELECT count(*) FROM generation_jobs WHERE guest_session_id=$1 AND state IN ('queued','running')")
            .bind(owner).fetch_one(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        let active_deployment: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM generation_jobs WHERE state IN ('queued','running')",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| JobError::Unavailable)?;
        let recent_for_guest: i64 = sqlx::query_scalar("SELECT count(*) FROM planning_requests WHERE guest_session_id=$1 AND created_at > now() - interval '1 minute'")
            .bind(owner).fetch_one(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        if active_for_guest >= self.limits.max_active_jobs_per_guest
            || active_deployment >= self.limits.max_active_jobs_deployment
            || recent_for_guest >= self.limits.max_submissions_per_guest_per_minute
        {
            return Err(JobError::RateLimited {
                retry_after_seconds: 60,
            });
        }
        let request_id = Uuid::new_v4();
        let inserted = sqlx::query("INSERT INTO planning_requests (id, guest_session_id, resolved_start_id, revision, idempotency_key, input_hash, requested_pin, requested_distance_metres, distance_tolerance_metres, activity_profile, activity_profile_version, coverage_policy_version, template_id, template_version, map_data_version, algorithm_version, return_to_start, expires_at) SELECT $1, $2, rs.id, $3, $4, $5, rs.confirmed_start, $6, $7, $8::jsonb, $9, $10, $11, $12, $13, $14, $15, $16 FROM resolved_starts rs WHERE rs.id = $17 AND rs.guest_session_id = $2 AND rs.expires_at > now() AND rs.profile_version = $9 AND rs.coverage_policy_version = $10 AND rs.map_data_version = $13 RETURNING id")
            .bind(request_id).bind(owner).bind(input.revision).bind(&input.idempotency_key)
            .bind(input.input_hash.as_slice()).bind(input.requested_distance_metres).bind(input.distance_tolerance_metres)
            .bind(&input.activity_profile_json).bind(&input.activity_profile_version).bind(&input.coverage_policy_version)
            .bind(&input.template_id).bind(&input.template_version).bind(&input.map_data_version)
            .bind(&input.algorithm_version).bind(input.return_to_start).bind(OffsetDateTime::from(input.expires_at)).bind(input.resolved_start_id)
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
        let queue_health = sqlx::query("SELECT count(*) FILTER (WHERE state='queued') AS backlog, count(*) FILTER (WHERE state='running' AND lease_expires_at <= $1) AS stuck FROM generation_jobs WHERE state IN ('queued','running')")
            .bind(OffsetDateTime::from(now))
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| JobError::Unavailable)?;
        let backlog: i64 = queue_health
            .try_get("backlog")
            .map_err(|_| JobError::Unavailable)?;
        let stuck: i64 = queue_health
            .try_get("stuck")
            .map_err(|_| JobError::Unavailable)?;
        sqlx::query("WITH terminal AS (UPDATE generation_jobs SET state=(CASE WHEN deadline_at <= $1 THEN 'timed_out' ELSE 'failed' END)::generation_job_state, completed_at=$1, lease_owner=NULL, lease_expires_at=NULL, status_version=status_version+1 WHERE state IN ('queued','running') AND (lease_expires_at IS NULL OR lease_expires_at <= $1) AND (deadline_at <= $1 OR attempt_count >= max_attempts) RETURNING id,state) INSERT INTO generation_job_outcomes (generation_job_id,outcome,public_code,budget_exhausted) SELECT id,state::text::generation_terminal_outcome,CASE WHEN state='timed_out' THEN 'timed_out' ELSE 'attempts_exhausted' END,state='timed_out' FROM terminal")
            .bind(OffsetDateTime::from(now)).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        let row = sqlx::query("WITH claimable AS (SELECT id FROM generation_jobs WHERE state IN ('queued','running') AND deadline_at > $1 AND attempt_count < max_attempts AND (lease_expires_at IS NULL OR lease_expires_at <= $1) ORDER BY queued_at FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE generation_jobs j SET state='running', stage=CASE WHEN j.state='queued' THEN 'checking_roads' ELSE j.stage END, status_version=status_version+1, started_at=COALESCE(started_at,$1), lease_owner=$2, lease_expires_at=$3, attempt_count=attempt_count+1 FROM claimable WHERE j.id=claimable.id RETURNING j.id,j.attempt_count,j.lease_expires_at,j.queued_at")
            .bind(OffsetDateTime::from(now)).bind(worker).bind(OffsetDateTime::from(now + lease_for))
            .fetch_optional(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        let Some(row) = row else {
            tx.commit().await.map_err(|_| JobError::Unavailable)?;
            emit_queue_health(backlog, stuck);
            return Ok(None);
        };
        let job_id: Uuid = row.try_get("id").map_err(|_| JobError::Unavailable)?;
        let attempt: i32 = row
            .try_get("attempt_count")
            .map_err(|_| JobError::Unavailable)?;
        let expires: OffsetDateTime = row
            .try_get("lease_expires_at")
            .map_err(|_| JobError::Unavailable)?;
        let queued_at: OffsetDateTime = row
            .try_get("queued_at")
            .map_err(|_| JobError::Unavailable)?;
        sqlx::query("INSERT INTO job_attempts (id,generation_job_id,attempt_number,worker_id) VALUES ($1,$2,$3,$4)")
            .bind(Uuid::new_v4()).bind(job_id).bind(attempt).bind(worker).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        tx.commit().await.map_err(|_| JobError::Unavailable)?;
        emit_queue_health(backlog, stuck);
        MetricEvent::duration(
            MetricName::QueueDelay,
            Correlation::for_job(job_id).with_attempt(attempt as u32),
            now.duration_since(SystemTime::from(queued_at))
                .unwrap_or_default(),
            None,
        )
        .emit();
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

    pub async fn advance_stage(
        &self,
        lease: &JobLease,
        now: SystemTime,
        stage: JobStage,
    ) -> Result<i64, JobError> {
        let allowed_current = match stage {
            JobStage::CheckingRoads => vec![JobStage::CheckingRoads.as_str()],
            JobStage::FittingDesign => vec![
                JobStage::CheckingRoads.as_str(),
                JobStage::FittingDesign.as_str(),
            ],
            JobStage::ComparingRoutes => vec![
                JobStage::FittingDesign.as_str(),
                JobStage::ComparingRoutes.as_str(),
            ],
            JobStage::Queued => return Err(JobError::InvalidInput),
        };
        sqlx::query_scalar("UPDATE generation_jobs SET status_version=status_version+CASE WHEN stage=$1 THEN 0 ELSE 1 END, stage=$1 WHERE id=$2 AND state='running' AND lease_owner=$3 AND attempt_count=$4 AND lease_expires_at>$5 AND deadline_at>$5 AND stage=ANY($6) RETURNING status_version")
            .bind(stage.as_str())
            .bind(lease.job_id)
            .bind(lease.worker_id)
            .bind(lease.attempt as i32)
            .bind(OffsetDateTime::from(now))
            .bind(allowed_current)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| JobError::Unavailable)?
            .ok_or(JobError::LeaseLost)
    }

    pub async fn status(&self, owner: Uuid, job_id: Uuid) -> Result<JobStatusSnapshot, JobError> {
        let row = sqlx::query("SELECT gj.id, pr.revision, gj.state::text AS state, gj.stage, gj.status_version, gj.queued_at, gj.started_at, gj.completed_at, gj.deadline_at, pr.expires_at, outcome.public_code, outcome.budget_exhausted, COALESCE(outcome.result_ids, '{}'::uuid[]) AS result_ids FROM generation_jobs gj JOIN planning_requests pr ON pr.id=gj.planning_request_id LEFT JOIN generation_job_outcomes outcome ON outcome.generation_job_id=gj.id WHERE gj.id=$1 AND gj.guest_session_id=$2")
            .bind(job_id)
            .bind(owner)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| JobError::Unavailable)?
            .ok_or(JobError::NotFound)?;
        let state_name: String = row.try_get("state").map_err(|_| JobError::Unavailable)?;
        let state = parse_state(&state_name)?;
        let stage_name: String = row.try_get("stage").map_err(|_| JobError::Unavailable)?;
        let stage = parse_stage(&stage_name)?;
        let public_code: Option<String> = row
            .try_get("public_code")
            .map_err(|_| JobError::Unavailable)?;
        let outcome = public_code
            .map(|code| {
                Ok(JobOutcomeSnapshot {
                    code: parse_outcome_code(&code)?,
                    budget_exhausted: row
                        .try_get::<Option<bool>, _>("budget_exhausted")
                        .map_err(|_| JobError::Unavailable)?
                        .unwrap_or(false),
                })
            })
            .transpose()?;
        if state.is_terminal() != outcome.is_some()
            || outcome
                .as_ref()
                .is_some_and(|value| !value.code.matches_state(state))
        {
            return Err(JobError::Unavailable);
        }
        let result_ids: Vec<Uuid> = row
            .try_get("result_ids")
            .map_err(|_| JobError::Unavailable)?;
        if (state == JobState::Succeeded) != !result_ids.is_empty() {
            return Err(JobError::Unavailable);
        }
        Ok(JobStatusSnapshot {
            job_id: row.try_get("id").map_err(|_| JobError::Unavailable)?,
            request_revision: row.try_get("revision").map_err(|_| JobError::Unavailable)?,
            state,
            stage,
            status_version: row
                .try_get("status_version")
                .map_err(|_| JobError::Unavailable)?,
            queued_at: SystemTime::from(
                row.try_get::<OffsetDateTime, _>("queued_at")
                    .map_err(|_| JobError::Unavailable)?,
            ),
            started_at: row
                .try_get::<Option<OffsetDateTime>, _>("started_at")
                .map_err(|_| JobError::Unavailable)?
                .map(SystemTime::from),
            completed_at: row
                .try_get::<Option<OffsetDateTime>, _>("completed_at")
                .map_err(|_| JobError::Unavailable)?
                .map(SystemTime::from),
            deadline_at: SystemTime::from(
                row.try_get::<OffsetDateTime, _>("deadline_at")
                    .map_err(|_| JobError::Unavailable)?,
            ),
            expires_at: SystemTime::from(
                row.try_get::<OffsetDateTime, _>("expires_at")
                    .map_err(|_| JobError::Unavailable)?,
            ),
            outcome,
            result_ids,
        })
    }

    pub async fn begin_provider_call(
        &self,
        lease: &JobLease,
        now: SystemTime,
        required: Duration,
    ) -> Result<Option<PostgresProviderCallPermit>, JobError> {
        let required_until = now.checked_add(required).ok_or(JobError::InvalidInput)?;
        let mut transaction = self.pool.begin().await.map_err(|_| JobError::Unavailable)?;
        let row = sqlx::query("SELECT state::text AS state, cancellation_requested_at, lease_owner, attempt_count, lease_expires_at, deadline_at, provider_call_count FROM generation_jobs WHERE id=$1 FOR UPDATE")
            .bind(lease.job_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| JobError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let state: String = row.try_get("state").map_err(|_| JobError::Unavailable)?;
        let cancellation_requested_at: Option<OffsetDateTime> = row
            .try_get("cancellation_requested_at")
            .map_err(|_| JobError::Unavailable)?;
        let lease_owner: Option<Uuid> = row
            .try_get("lease_owner")
            .map_err(|_| JobError::Unavailable)?;
        let attempt: i32 = row
            .try_get("attempt_count")
            .map_err(|_| JobError::Unavailable)?;
        let lease_expires_at: Option<OffsetDateTime> = row
            .try_get("lease_expires_at")
            .map_err(|_| JobError::Unavailable)?;
        let deadline_at: OffsetDateTime = row
            .try_get("deadline_at")
            .map_err(|_| JobError::Unavailable)?;
        let provider_call_count: i32 = row
            .try_get("provider_call_count")
            .map_err(|_| JobError::Unavailable)?;
        let allowed = state == "running"
            && cancellation_requested_at.is_none()
            && lease_owner == Some(lease.worker_id)
            && attempt == lease.attempt as i32
            && lease_expires_at.is_some_and(|expires| expires > OffsetDateTime::from(now))
            && deadline_at >= OffsetDateTime::from(required_until)
            && usize::try_from(provider_call_count)
                .is_ok_and(|count| count < self.limits.max_provider_requests);
        if !allowed {
            return Ok(None);
        }
        sqlx::query(
            "UPDATE generation_jobs SET provider_call_count=provider_call_count+1 WHERE id=$1",
        )
        .bind(lease.job_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| JobError::Unavailable)?;
        Ok(Some(PostgresProviderCallPermit {
            transaction: Some(transaction),
        }))
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
            return parse_state(&state);
        }
        sqlx::query("UPDATE generation_jobs SET state='cancelled', status_version=status_version+1, cancellation_requested_at=$1, completed_at=$1, lease_owner=NULL, lease_expires_at=NULL WHERE id=$2")
            .bind(OffsetDateTime::from(now)).bind(job_id).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        sqlx::query("INSERT INTO generation_job_outcomes (generation_job_id,outcome,public_code) VALUES ($1,'cancelled','cancelled')")
            .bind(job_id).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        tx.commit().await.map_err(|_| JobError::Unavailable)?;
        MetricEvent::count(
            MetricName::Cancellation,
            Correlation::for_job(job_id),
            1,
            None,
        )
        .with_result_class("cancelled")
        .emit();
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
            "SELECT state::text AS state, deadline_at, queued_at FROM generation_jobs WHERE id=$1 FOR UPDATE",
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
        let deadline_at = SystemTime::from(
            row.try_get::<OffsetDateTime, _>("deadline_at")
                .map_err(|_| JobError::Unavailable)?,
        );
        let queued_at = SystemTime::from(
            row.try_get::<OffsetDateTime, _>("queued_at")
                .map_err(|_| JobError::Unavailable)?,
        );
        let (next, public_code, budget_exhausted, result_ids) = match completion {
            Completion::Succeeded {
                candidate_ids,
                budget_exhausted,
            } if !candidate_ids.is_empty() => {
                let count: i64 = sqlx::query_scalar("SELECT count(*) FROM route_candidates rc JOIN generation_jobs gj ON gj.id=rc.generation_job_id JOIN planning_requests pr ON pr.id=gj.planning_request_id WHERE rc.generation_job_id=$1 AND rc.id=ANY($2) AND rc.map_data_version=pr.map_data_version AND rc.algorithm_version=pr.algorithm_version")
                    .bind(lease.job_id).bind(candidate_ids).fetch_one(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
                if count == candidate_ids.len() as i64 {
                    (
                        JobState::Succeeded,
                        JobOutcomeCode::Succeeded,
                        *budget_exhausted,
                        candidate_ids.clone(),
                    )
                } else if now >= deadline_at {
                    (
                        JobState::TimedOut,
                        JobOutcomeCode::TimedOut,
                        true,
                        Vec::new(),
                    )
                } else {
                    return Err(JobError::InvalidInput);
                }
            }
            Completion::Succeeded { .. } | Completion::TimedOut if now >= deadline_at => (
                JobState::TimedOut,
                JobOutcomeCode::TimedOut,
                true,
                Vec::new(),
            ),
            Completion::NoMatch { code }
                if code.matches_state(JobState::NoMatch)
                    && *code != JobOutcomeCode::LegacyUnknown =>
            {
                (
                    JobState::NoMatch,
                    *code,
                    matches!(code, JobOutcomeCode::ExhaustedBudget),
                    Vec::new(),
                )
            }
            Completion::Failed { code }
                if code.matches_state(JobState::Failed)
                    && *code != JobOutcomeCode::LegacyUnknown =>
            {
                (JobState::Failed, *code, false, Vec::new())
            }
            Completion::Succeeded { .. }
            | Completion::NoMatch { .. }
            | Completion::Failed { .. }
            | Completion::TimedOut => {
                return Err(JobError::InvalidInput);
            }
        };
        let state_name = match next {
            JobState::Succeeded => "succeeded",
            JobState::NoMatch => "no_match",
            JobState::Failed => "failed",
            JobState::TimedOut => "timed_out",
            JobState::Queued | JobState::Running | JobState::Cancelled => {
                return Err(JobError::InvalidInput);
            }
        };
        sqlx::query("UPDATE generation_jobs SET state=$1::generation_job_state, status_version=status_version+1, completed_at=$2, lease_owner=NULL, lease_expires_at=NULL WHERE id=$3")
            .bind(state_name).bind(OffsetDateTime::from(now)).bind(lease.job_id).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        sqlx::query("UPDATE job_attempts SET finished_at=$1,result_code=$2 WHERE generation_job_id=$3 AND attempt_number=$4")
            .bind(OffsetDateTime::from(now)).bind(state_name).bind(lease.job_id).bind(lease.attempt as i32).execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        sqlx::query("INSERT INTO generation_job_outcomes (generation_job_id,outcome,public_code,budget_exhausted,result_ids) VALUES ($1,$2::generation_terminal_outcome,$3,$4,$5)")
            .bind(lease.job_id).bind(state_name).bind(public_code.as_str()).bind(budget_exhausted || matches!(next, JobState::Succeeded) && now >= deadline_at).bind(&result_ids)
            .execute(&mut *tx).await.map_err(|_| JobError::Unavailable)?;
        tx.commit().await.map_err(|_| JobError::Unavailable)?;
        let correlation = Correlation::for_job(lease.job_id).with_attempt(lease.attempt);
        MetricEvent::duration(
            MetricName::TotalLatency,
            correlation,
            now.duration_since(queued_at).unwrap_or_default(),
            None,
        )
        .with_result_class(public_code.as_str())
        .emit();
        MetricEvent::count(MetricName::TerminalOutcome, correlation, 1, None)
            .with_result_class(public_code.as_str())
            .emit();
        MetricEvent::count(
            MetricName::CandidatesReturned,
            correlation,
            result_ids.len() as u64,
            None,
        )
        .with_result_class(public_code.as_str())
        .emit();
        if matches!(next, JobState::TimedOut) {
            MetricEvent::count(MetricName::Timeout, correlation, 1, None)
                .with_result_class("timed_out")
                .emit();
        }
        Ok(next)
    }
}

fn emit_queue_health(backlog: i64, stuck: i64) {
    let correlation = Correlation::system();
    MetricEvent::count(
        MetricName::QueueBacklog,
        correlation,
        backlog.max(0) as u64,
        None,
    )
    .emit();
    MetricEvent::count(
        MetricName::StuckLeases,
        correlation,
        stuck.max(0) as u64,
        None,
    )
    .emit();
}

fn parse_state(state: &str) -> Result<JobState, JobError> {
    match state {
        "queued" => Ok(JobState::Queued),
        "running" => Ok(JobState::Running),
        "succeeded" => Ok(JobState::Succeeded),
        "no_match" => Ok(JobState::NoMatch),
        "failed" => Ok(JobState::Failed),
        "cancelled" => Ok(JobState::Cancelled),
        "timed_out" => Ok(JobState::TimedOut),
        _ => Err(JobError::Unavailable),
    }
}

fn parse_stage(stage: &str) -> Result<JobStage, JobError> {
    match stage {
        "queued" => Ok(JobStage::Queued),
        "checking_roads" => Ok(JobStage::CheckingRoads),
        "fitting_design" => Ok(JobStage::FittingDesign),
        "comparing_routes" => Ok(JobStage::ComparingRoutes),
        _ => Err(JobError::Unavailable),
    }
}

fn parse_outcome_code(code: &str) -> Result<JobOutcomeCode, JobError> {
    match code {
        "succeeded" => Ok(JobOutcomeCode::Succeeded),
        "unsupported_coverage" => Ok(JobOutcomeCode::UnsupportedCoverage),
        "no_routable_access" => Ok(JobOutcomeCode::NoRoutableAccess),
        "disconnected_network" => Ok(JobOutcomeCode::DisconnectedNetwork),
        "insufficient_visual_match" => Ok(JobOutcomeCode::InsufficientVisualMatch),
        "outside_distance" => Ok(JobOutcomeCode::OutsideDistance),
        "exhausted_budget" => Ok(JobOutcomeCode::ExhaustedBudget),
        "provider_unavailable" => Ok(JobOutcomeCode::ProviderUnavailable),
        "attempts_exhausted" => Ok(JobOutcomeCode::AttemptsExhausted),
        "cancelled" => Ok(JobOutcomeCode::Cancelled),
        "timed_out" => Ok(JobOutcomeCode::TimedOut),
        "legacy_unknown" => Ok(JobOutcomeCode::LegacyUnknown),
        _ => Err(JobError::Unavailable),
    }
}
