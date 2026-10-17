use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use std::{
    collections::BTreeMap,
    fmt,
    time::{Duration, SystemTime},
};
use thiserror::Error;
use time::OffsetDateTime;
use tokio::time::timeout;
use uuid::Uuid;

pub mod jobs;
pub mod routes;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PrivateResourceKind {
    ResolvedStart,
    Job,
    Route,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuestSessionRecord {
    pub id: Uuid,
    pub token_digest: [u8; 32],
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
    pub cleanup_after: SystemTime,
    pub revoked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedStartRecord {
    pub id: Uuid,
    pub guest_session_id: Uuid,
    pub requested_longitude: f64,
    pub requested_latitude: f64,
    pub confirmed_longitude: f64,
    pub confirmed_latitude: f64,
    pub profile_version: String,
    pub coverage_policy_version: String,
    pub map_data_version: String,
    pub created_at: SystemTime,
    pub expires_at: SystemTime,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ResolvedStartStoreError {
    #[error("resolved-start storage is unavailable")]
    Unavailable,
}

#[async_trait]
pub trait ResolvedStartStore: Send + Sync + 'static {
    async fn insert_resolved_start(
        &self,
        record: ResolvedStartRecord,
    ) -> Result<(), ResolvedStartStoreError>;
}

#[derive(Clone)]
pub struct PostgresResolvedStartStore {
    pool: PgPool,
}

impl PostgresResolvedStartStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str) -> Result<Self, ResolvedStartStoreError> {
        PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map(Self::new)
            .map_err(|_| ResolvedStartStoreError::Unavailable)
    }
}

#[async_trait]
impl ResolvedStartStore for PostgresResolvedStartStore {
    async fn insert_resolved_start(
        &self,
        record: ResolvedStartRecord,
    ) -> Result<(), ResolvedStartStoreError> {
        let inserted = sqlx::query(
            "INSERT INTO resolved_starts (id, guest_session_id, requested_pin, confirmed_start, profile_version, coverage_policy_version, map_data_version, created_at, expires_at) VALUES ($1,$2,ST_SetSRID(ST_MakePoint($3,$4),4326),ST_SetSRID(ST_MakePoint($5,$6),4326),$7,$8,$9,$10,$11) ON CONFLICT (id) DO NOTHING",
        )
        .bind(record.id)
        .bind(record.guest_session_id)
        .bind(record.requested_longitude)
        .bind(record.requested_latitude)
        .bind(record.confirmed_longitude)
        .bind(record.confirmed_latitude)
        .bind(&record.profile_version)
        .bind(&record.coverage_policy_version)
        .bind(&record.map_data_version)
        .bind(OffsetDateTime::from(record.created_at))
        .bind(OffsetDateTime::from(record.expires_at))
        .execute(&self.pool)
        .await
        .map_err(|_| ResolvedStartStoreError::Unavailable)?
        .rows_affected();
        if inserted == 1 {
            return Ok(());
        }
        let identical: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM resolved_starts WHERE id=$1 AND guest_session_id=$2 AND ST_X(requested_pin)=$3 AND ST_Y(requested_pin)=$4 AND ST_X(confirmed_start)=$5 AND ST_Y(confirmed_start)=$6 AND profile_version=$7 AND coverage_policy_version=$8 AND map_data_version=$9 AND expires_at=$10)",
        )
        .bind(record.id)
        .bind(record.guest_session_id)
        .bind(record.requested_longitude)
        .bind(record.requested_latitude)
        .bind(record.confirmed_longitude)
        .bind(record.confirmed_latitude)
        .bind(&record.profile_version)
        .bind(&record.coverage_policy_version)
        .bind(&record.map_data_version)
        .bind(OffsetDateTime::from(record.expires_at))
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ResolvedStartStoreError::Unavailable)?;
        if identical {
            Ok(())
        } else {
            Err(ResolvedStartStoreError::Unavailable)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CleanupReport {
    pub deleted_sessions: u64,
    pub deleted_jobs: u64,
    pub deleted_candidates: u64,
    pub deleted_starts: u64,
    pub retry_required: bool,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SessionStoreError {
    #[error("session storage is unavailable")]
    Unavailable,
}

#[async_trait]
pub trait GuestSessionStore: Send + Sync + 'static {
    async fn insert_session(&self, record: GuestSessionRecord) -> Result<(), SessionStoreError>;
    async fn find_session(
        &self,
        digest: [u8; 32],
    ) -> Result<Option<GuestSessionRecord>, SessionStoreError>;
    async fn revoke_session(&self, id: Uuid) -> Result<(), SessionStoreError>;
    async fn rotate_session(
        &self,
        previous_id: Uuid,
        replacement: GuestSessionRecord,
    ) -> Result<(), SessionStoreError>;
    async fn owns_resource(
        &self,
        session_id: Uuid,
        kind: PrivateResourceKind,
        resource_id: Uuid,
    ) -> Result<bool, SessionStoreError>;
    async fn cleanup_expired_sessions(
        &self,
        cutoff: SystemTime,
    ) -> Result<CleanupReport, SessionStoreError>;
}

#[derive(Clone)]
pub struct PostgresGuestSessionStore {
    pool: PgPool,
}

impl PostgresGuestSessionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str) -> Result<Self, SessionStoreError> {
        PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map(Self::new)
            .map_err(|_| SessionStoreError::Unavailable)
    }
}

#[async_trait]
impl GuestSessionStore for PostgresGuestSessionStore {
    async fn insert_session(&self, record: GuestSessionRecord) -> Result<(), SessionStoreError> {
        sqlx::query("INSERT INTO guest_sessions (id, token_hash, created_at, expires_at, cleanup_after) VALUES ($1, $2, $3, $4, $5)")
            .bind(record.id).bind(record.token_digest.as_slice()).bind(OffsetDateTime::from(record.created_at)).bind(OffsetDateTime::from(record.expires_at)).bind(OffsetDateTime::from(record.cleanup_after))
            .execute(&self.pool).await.map(|_| ()).map_err(|_| SessionStoreError::Unavailable)
    }

    async fn find_session(
        &self,
        digest: [u8; 32],
    ) -> Result<Option<GuestSessionRecord>, SessionStoreError> {
        let row = sqlx::query("SELECT id, token_hash, created_at, expires_at, cleanup_after, revoked_at IS NOT NULL AS revoked FROM guest_sessions WHERE token_hash = $1")
            .bind(digest.as_slice()).fetch_optional(&self.pool).await.map_err(|_| SessionStoreError::Unavailable)?;
        row.map(|row| {
            let stored: Vec<u8> = row
                .try_get("token_hash")
                .map_err(|_| SessionStoreError::Unavailable)?;
            let token_digest: [u8; 32] = stored
                .try_into()
                .map_err(|_| SessionStoreError::Unavailable)?;
            Ok(GuestSessionRecord {
                id: row
                    .try_get("id")
                    .map_err(|_| SessionStoreError::Unavailable)?,
                token_digest,
                created_at: SystemTime::from(
                    row.try_get::<OffsetDateTime, _>("created_at")
                        .map_err(|_| SessionStoreError::Unavailable)?,
                ),
                expires_at: SystemTime::from(
                    row.try_get::<OffsetDateTime, _>("expires_at")
                        .map_err(|_| SessionStoreError::Unavailable)?,
                ),
                cleanup_after: SystemTime::from(
                    row.try_get::<OffsetDateTime, _>("cleanup_after")
                        .map_err(|_| SessionStoreError::Unavailable)?,
                ),
                revoked: row
                    .try_get("revoked")
                    .map_err(|_| SessionStoreError::Unavailable)?,
            })
        })
        .transpose()
    }

    async fn revoke_session(&self, id: Uuid) -> Result<(), SessionStoreError> {
        sqlx::query(
            "UPDATE guest_sessions SET revoked_at = COALESCE(revoked_at, now()) WHERE id = $1",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(|_| SessionStoreError::Unavailable)
    }

    async fn rotate_session(
        &self,
        previous_id: Uuid,
        replacement: GuestSessionRecord,
    ) -> Result<(), SessionStoreError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| SessionStoreError::Unavailable)?;
        let revoked = sqlx::query("UPDATE guest_sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL AND expires_at > now()")
            .bind(previous_id).execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?;
        if revoked.rows_affected() != 1 {
            return Err(SessionStoreError::Unavailable);
        }
        sqlx::query("INSERT INTO guest_sessions (id, token_hash, created_at, expires_at, cleanup_after) VALUES ($1, $2, $3, $4, $5)")
            .bind(replacement.id).bind(replacement.token_digest.as_slice()).bind(OffsetDateTime::from(replacement.created_at)).bind(OffsetDateTime::from(replacement.expires_at)).bind(OffsetDateTime::from(replacement.cleanup_after))
            .execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| SessionStoreError::Unavailable)
    }

    async fn owns_resource(
        &self,
        session_id: Uuid,
        kind: PrivateResourceKind,
        resource_id: Uuid,
    ) -> Result<bool, SessionStoreError> {
        let owned = match kind {
            PrivateResourceKind::ResolvedStart => sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM resolved_starts WHERE id = $1 AND guest_session_id = $2)"),
            PrivateResourceKind::Job => sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE id = $1 AND guest_session_id = $2)"),
            PrivateResourceKind::Route => sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM route_candidates WHERE id = $1 AND guest_session_id = $2)"),
        }.bind(resource_id).bind(session_id).fetch_one(&self.pool).await.map_err(|_| SessionStoreError::Unavailable)?;
        Ok(owned)
    }

    async fn cleanup_expired_sessions(
        &self,
        cutoff: SystemTime,
    ) -> Result<CleanupReport, SessionStoreError> {
        const BATCH_SIZE: i64 = 100;
        let cutoff = OffsetDateTime::from(cutoff);
        let run_id = Uuid::new_v4();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| SessionStoreError::Unavailable)?;
        sqlx::query("INSERT INTO cleanup_runs (id, cutoff_at, status) VALUES ($1,$2,'running')")
            .bind(run_id)
            .bind(cutoff)
            .execute(&mut *transaction)
            .await
            .map_err(|_| SessionStoreError::Unavailable)?;

        let eligible = "SELECT pr.id FROM planning_requests pr WHERE pr.expires_at <= $1 AND NOT EXISTS (SELECT 1 FROM generation_jobs active WHERE active.planning_request_id=pr.id AND active.state='running' AND active.lease_expires_at>$1) ORDER BY pr.expires_at LIMIT $2";
        let deleted_candidates = sqlx::query(&format!("WITH eligible AS ({eligible}) DELETE FROM route_candidates rc USING generation_jobs gj WHERE rc.generation_job_id=gj.id AND gj.planning_request_id IN (SELECT id FROM eligible)"))
            .bind(cutoff).bind(BATCH_SIZE).execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?.rows_affected();
        let deleted_jobs = sqlx::query(&format!("WITH eligible AS ({eligible}) DELETE FROM generation_jobs gj WHERE gj.planning_request_id IN (SELECT id FROM eligible)"))
            .bind(cutoff).bind(BATCH_SIZE).execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?.rows_affected();
        sqlx::query(&format!("WITH eligible AS ({eligible}) DELETE FROM planning_requests pr WHERE pr.id IN (SELECT id FROM eligible)"))
            .bind(cutoff).bind(BATCH_SIZE).execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?;
        let deleted_starts = sqlx::query("WITH eligible AS (SELECT rs.id FROM resolved_starts rs WHERE rs.expires_at <= $1 AND NOT EXISTS (SELECT 1 FROM planning_requests pr WHERE pr.resolved_start_id=rs.id) ORDER BY rs.expires_at LIMIT $2) DELETE FROM resolved_starts rs WHERE rs.id IN (SELECT id FROM eligible)")
            .bind(cutoff).bind(BATCH_SIZE).execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?.rows_affected();
        let deleted_sessions = sqlx::query("WITH eligible AS (SELECT gs.id FROM guest_sessions gs WHERE gs.cleanup_after <= $1 AND NOT EXISTS (SELECT 1 FROM planning_requests pr WHERE pr.guest_session_id=gs.id) AND NOT EXISTS (SELECT 1 FROM resolved_starts rs WHERE rs.guest_session_id=gs.id) ORDER BY gs.cleanup_after LIMIT $2) DELETE FROM guest_sessions gs WHERE gs.id IN (SELECT id FROM eligible)")
            .bind(cutoff).bind(BATCH_SIZE).execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?.rows_affected();
        let retry_required: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM planning_requests WHERE expires_at <= $1) OR EXISTS(SELECT 1 FROM resolved_starts WHERE expires_at <= $1) OR EXISTS(SELECT 1 FROM guest_sessions WHERE cleanup_after <= $1)")
            .bind(cutoff).fetch_one(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?;
        sqlx::query("UPDATE cleanup_runs SET finished_at=now(), status='succeeded', deleted_session_count=$1, deleted_job_count=$2, deleted_candidate_count=$3, deleted_start_count=$4, retry_required=$5, retry_after=CASE WHEN $5 THEN now()+interval '30 seconds' ELSE NULL END WHERE id=$6")
            .bind(i32::try_from(deleted_sessions).map_err(|_| SessionStoreError::Unavailable)?)
            .bind(i32::try_from(deleted_jobs).map_err(|_| SessionStoreError::Unavailable)?)
            .bind(i32::try_from(deleted_candidates).map_err(|_| SessionStoreError::Unavailable)?)
            .bind(i32::try_from(deleted_starts).map_err(|_| SessionStoreError::Unavailable)?)
            .bind(retry_required).bind(run_id).execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| SessionStoreError::Unavailable)?;
        Ok(CleanupReport {
            deleted_sessions,
            deleted_jobs,
            deleted_candidates,
            deleted_starts,
            retry_required,
        })
    }
}

#[async_trait]
pub trait DependencyProbe: Send + Sync {
    async fn check(&self) -> Result<(), DependencyError>;
}

pub struct PostgresProbe {
    database_url: String,
    timeout: Duration,
}

impl PostgresProbe {
    pub fn new(database_url: String, timeout: Duration) -> Self {
        Self {
            database_url,
            timeout,
        }
    }

    async fn connect(&self) -> Result<PgPool, DependencyError> {
        timeout(
            self.timeout,
            PgPoolOptions::new()
                .max_connections(1)
                .acquire_timeout(self.timeout)
                .connect(&self.database_url),
        )
        .await
        .map_err(|_| DependencyError::PostgresUnavailable)?
        .map_err(|_| DependencyError::PostgresUnavailable)
    }
}

impl fmt::Debug for PostgresProbe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresProbe")
            .field("database_url", &"[REDACTED]")
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[async_trait]
impl DependencyProbe for PostgresProbe {
    async fn check(&self) -> Result<(), DependencyError> {
        let pool = self.connect().await?;
        timeout(self.timeout, sqlx::query("SELECT 1").execute(&pool))
            .await
            .map_err(|_| DependencyError::PostgresUnavailable)?
            .map_err(|_| DependencyError::PostgresUnavailable)?;
        match migration_status(&pool).await? {
            MigrationStatus::Current => Ok(()),
            MigrationStatus::Missing | MigrationStatus::Pending { .. } => {
                Err(DependencyError::MigrationsNotCurrent)
            }
            MigrationStatus::Incompatible { .. } => Err(DependencyError::MigrationsIncompatible),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum MigrationStatus {
    Current,
    Missing,
    Pending { versions: Vec<i64> },
    Incompatible { version: i64 },
}

pub async fn migration_status(pool: &PgPool) -> Result<MigrationStatus, DependencyError> {
    let migration_table_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .map_err(|_| DependencyError::MigrationsIncompatible)?;
    if !migration_table_exists {
        return Ok(MigrationStatus::Missing);
    }

    let rows =
        sqlx::query("SELECT version, success, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(pool)
            .await
            .map_err(|_| DependencyError::MigrationsIncompatible)?;
    let applied = rows
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get::<i64, _>("version")?,
                (
                    row.try_get::<bool, _>("success")?,
                    row.try_get::<Vec<u8>, _>("checksum")?,
                ),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, sqlx::Error>>()
        .map_err(|_| DependencyError::MigrationsIncompatible)?;

    for (version, (success, checksum)) in &applied {
        let Some(expected) = MIGRATOR
            .iter()
            .find(|migration| migration.version == *version)
        else {
            return Ok(MigrationStatus::Incompatible { version: *version });
        };
        if !success || checksum.as_slice() != expected.checksum.as_ref() {
            return Ok(MigrationStatus::Incompatible { version: *version });
        }
    }
    let pending = MIGRATOR
        .iter()
        .filter(|migration| !applied.contains_key(&migration.version))
        .map(|migration| migration.version)
        .collect::<Vec<_>>();
    if pending.is_empty() {
        Ok(MigrationStatus::Current)
    } else {
        Ok(MigrationStatus::Pending { versions: pending })
    }
}

pub async fn apply_migrations(database_url: &str) -> Result<(), MigrationApplyError> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    run_migrations(&pool).await?;
    pool.close().await;
    Ok(())
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), MigrationApplyError> {
    prepare_populated_v4_upgrade(pool).await?;
    MIGRATOR.run(pool).await?;
    Ok(())
}

async fn prepare_populated_v4_upgrade(pool: &PgPool) -> Result<(), sqlx::Error> {
    let migration_table_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await?;
    if !migration_table_exists {
        return Ok(());
    }
    let needs_repair: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version=4 AND success)
         AND NOT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version=5 AND success)",
    )
    .fetch_one(pool)
    .await?;
    if !needs_repair {
        return Ok(());
    }
    let mut transaction = pool.begin().await?;
    sqlx::raw_sql(
        "DROP TRIGGER generation_jobs_terminal_outcome_check ON generation_jobs;
         DROP TRIGGER generation_job_outcomes_state_check ON generation_job_outcomes;
         CREATE CONSTRAINT TRIGGER generation_jobs_terminal_outcome_check
         AFTER INSERT OR UPDATE OF state OR DELETE ON generation_jobs
         NOT DEFERRABLE FOR EACH ROW EXECUTE FUNCTION enforce_one_terminal_outcome();
         CREATE CONSTRAINT TRIGGER generation_job_outcomes_state_check
         AFTER INSERT OR UPDATE OR DELETE ON generation_job_outcomes
         NOT DEFERRABLE FOR EACH ROW EXECUTE FUNCTION enforce_one_terminal_outcome();",
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await
}

#[derive(Debug, Error)]
pub enum MigrationApplyError {
    #[error("database connection failed")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum DependencyError {
    #[error("PostgreSQL dependency is unavailable")]
    PostgresUnavailable,
    #[error("database migrations have not all been applied")]
    MigrationsNotCurrent,
    #[error("database migrations are incompatible with this build")]
    MigrationsIncompatible,
}

impl DependencyError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::PostgresUnavailable => "postgres_unavailable",
            Self::MigrationsNotCurrent => "migrations_not_current",
            Self::MigrationsIncompatible => "migrations_incompatible",
        }
    }
}
