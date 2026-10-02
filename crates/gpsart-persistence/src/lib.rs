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

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PrivateResourceKind {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CleanupReport {
    pub deleted_sessions: u64,
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
    pub async fn connect(database_url: &str) -> Result<Self, SessionStoreError> {
        PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map(|pool| Self { pool })
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
        sqlx::query("UPDATE guest_sessions SET revoked_at = COALESCE(revoked_at, now()) WHERE id = $1 AND revoked_at IS NULL")
            .bind(previous_id).execute(&mut *transaction).await.map_err(|_| SessionStoreError::Unavailable)?;
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
            PrivateResourceKind::Job => sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE id = $1 AND guest_session_id = $2)"),
            PrivateResourceKind::Route => sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM route_candidates WHERE id = $1 AND guest_session_id = $2)"),
        }.bind(resource_id).bind(session_id).fetch_one(&self.pool).await.map_err(|_| SessionStoreError::Unavailable)?;
        Ok(owned)
    }

    async fn cleanup_expired_sessions(
        &self,
        cutoff: SystemTime,
    ) -> Result<CleanupReport, SessionStoreError> {
        // Deliberately return only a count: cleanup must never materialize or log route geometry.
        let deleted_sessions = sqlx::query("DELETE FROM guest_sessions WHERE cleanup_after <= $1")
            .bind(OffsetDateTime::from(cutoff))
            .execute(&self.pool)
            .await
            .map_err(|_| SessionStoreError::Unavailable)?
            .rows_affected();
        Ok(CleanupReport { deleted_sessions })
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
    MIGRATOR.run(&pool).await?;
    pool.close().await;
    Ok(())
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
