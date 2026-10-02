use async_trait::async_trait;
use std::{fmt, time::Duration};
use thiserror::Error;
use tokio::time::timeout;
use tokio_postgres::NoTls;

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
        let connection = timeout(
            self.timeout,
            tokio_postgres::connect(&self.database_url, NoTls),
        )
        .await
        .map_err(|_| DependencyError::PostgresUnavailable)?
        .map_err(|_| DependencyError::PostgresUnavailable)?;
        let (client, connection) = connection;
        let task = tokio::spawn(async move {
            let _ = connection.await;
        });
        let query = timeout(self.timeout, client.simple_query("SELECT 1")).await;
        task.abort();
        match query {
            Ok(Ok(_)) => Ok(()),
            _ => Err(DependencyError::PostgresUnavailable),
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum DependencyError {
    #[error("PostgreSQL dependency is unavailable")]
    PostgresUnavailable,
}

impl DependencyError {
    pub const fn code(&self) -> &'static str {
        "postgres_unavailable"
    }
}
