use gpsart_persistence::jobs::{JobError, JobLease, PostgresJobCoordinator};
use gpsart_persistence::{DependencyProbe, PostgresProbe};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use tokio::sync::watch;

type Operation =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), WorkerError>> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct WorkerDependencies {
    check: Operation,
    take_work: Operation,
}

impl WorkerDependencies {
    pub fn postgres(probe: PostgresProbe) -> Self {
        let probe = Arc::new(probe);
        Self {
            check: Arc::new(move || {
                let probe = probe.clone();
                Box::pin(async move {
                    probe
                        .check()
                        .await
                        .map_err(|_| WorkerError::DependencyUnavailable)
                })
            }),
            take_work: Arc::new(|| Box::pin(async { Ok(()) })),
        }
    }

    pub fn for_test(checks: Arc<AtomicUsize>, leases: Arc<AtomicUsize>) -> Self {
        Self {
            check: Arc::new(move || {
                checks.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            }),
            take_work: Arc::new(move || {
                leases.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            }),
        }
    }
}

pub async fn run_until_shutdown(
    dependencies: WorkerDependencies,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), WorkerError> {
    (dependencies.check)().await?;
    loop {
        if *shutdown.borrow() {
            return Ok(());
        }
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() { return Ok(()); }
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                if *shutdown.borrow() { return Ok(()); }
                (dependencies.take_work)().await?;
            }
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum WorkerError {
    #[error("worker dependency is unavailable")]
    DependencyUnavailable,
    #[error("durable job state is unavailable")]
    JobStateUnavailable,
}

#[async_trait::async_trait]
pub trait ProviderGate: Send + Sync {
    async fn may_schedule(
        &self,
        lease: &JobLease,
        required_budget: Duration,
    ) -> Result<bool, JobError>;
}

#[async_trait::async_trait]
impl ProviderGate for PostgresJobCoordinator {
    async fn may_schedule(
        &self,
        lease: &JobLease,
        required_budget: Duration,
    ) -> Result<bool, JobError> {
        self.may_schedule_provider(lease, std::time::SystemTime::now(), required_budget)
            .await
    }
}

pub async fn run_provider_call<T>(
    gate: &dyn ProviderGate,
    lease: &JobLease,
    required_budget: Duration,
    operation: impl FnOnce() -> T,
) -> Result<Option<T>, WorkerError> {
    let allowed = gate
        .may_schedule(lease, required_budget)
        .await
        .map_err(|_| WorkerError::JobStateUnavailable)?;
    Ok(allowed.then(operation))
}
