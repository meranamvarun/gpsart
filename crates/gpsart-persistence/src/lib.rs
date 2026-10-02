use async_trait::async_trait;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use std::{collections::BTreeMap, fmt, time::Duration};
use thiserror::Error;
use tokio::time::timeout;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

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
