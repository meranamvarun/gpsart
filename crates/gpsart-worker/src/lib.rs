use gpsart_persistence::jobs::{
    JobError, JobLease, PostgresJobCoordinator, PostgresProviderCallPermit,
};
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
    #[error("provider task failed before producing a result")]
    ProviderTaskFailed,
}

#[async_trait::async_trait]
pub trait ProviderCallPermit: Send {
    async fn commit(self: Box<Self>) -> Result<(), JobError>;
}

#[async_trait::async_trait]
impl ProviderCallPermit for PostgresProviderCallPermit {
    async fn commit(self: Box<Self>) -> Result<(), JobError> {
        (*self).commit().await
    }
}

#[async_trait::async_trait]
pub trait ProviderGate: Send + Sync {
    async fn reserve(
        &self,
        lease: &JobLease,
        required_budget: Duration,
    ) -> Result<Option<Box<dyn ProviderCallPermit>>, JobError>;
}

#[async_trait::async_trait]
impl ProviderGate for PostgresJobCoordinator {
    async fn reserve(
        &self,
        lease: &JobLease,
        required_budget: Duration,
    ) -> Result<Option<Box<dyn ProviderCallPermit>>, JobError> {
        self.begin_provider_call(lease, std::time::SystemTime::now(), required_budget)
            .await
            .map(|permit| permit.map(|value| Box::new(value) as Box<dyn ProviderCallPermit>))
    }
}

pub async fn run_provider_call<T>(
    gate: &dyn ProviderGate,
    lease: &JobLease,
    required_budget: Duration,
    operation: impl FnOnce() -> tokio::task::JoinHandle<T>,
) -> Result<Option<T>, WorkerError> {
    let Some(permit) = gate
        .reserve(lease, required_budget)
        .await
        .map_err(|_| WorkerError::JobStateUnavailable)?
    else {
        return Ok(None);
    };
    let task = operation();
    permit
        .commit()
        .await
        .map_err(|_| WorkerError::JobStateUnavailable)?;
    task.await
        .map(Some)
        .map_err(|_| WorkerError::ProviderTaskFailed)
}
