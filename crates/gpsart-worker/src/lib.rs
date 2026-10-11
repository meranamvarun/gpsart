use gpsart_observability::{Correlation, MetricEvent, MetricName};
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
    time::{Duration, Instant},
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

    pub fn for_test_with_check_failures(
        checks: Arc<AtomicUsize>,
        leases: Arc<AtomicUsize>,
        failures_before_ready: usize,
    ) -> Self {
        Self {
            check: Arc::new(move || {
                let attempt = checks.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    if attempt < failures_before_ready {
                        Err(WorkerError::DependencyUnavailable)
                    } else {
                        Ok(())
                    }
                })
            }),
            take_work: Arc::new(move || {
                leases.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            }),
        }
    }

    pub fn for_test_with_work_failures(
        checks: Arc<AtomicUsize>,
        work_attempts: Arc<AtomicUsize>,
        failures_before_work: usize,
    ) -> Self {
        Self {
            check: Arc::new(move || {
                checks.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            }),
            take_work: Arc::new(move || {
                let attempt = work_attempts.fetch_add(1, Ordering::SeqCst);
                Box::pin(async move {
                    if attempt < failures_before_work {
                        Err(WorkerError::JobStateUnavailable)
                    } else {
                        Ok(())
                    }
                })
            }),
        }
    }
}

pub async fn run_until_shutdown(
    dependencies: WorkerDependencies,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), WorkerError> {
    'reconnect: loop {
        loop {
            match (dependencies.check)().await {
                Ok(()) => break,
                Err(WorkerError::DependencyUnavailable | WorkerError::JobStateUnavailable) => {
                    if *shutdown.borrow() {
                        return Ok(());
                    }
                    tokio::select! {
                        changed = shutdown.changed() => {
                            if changed.is_err() || *shutdown.borrow() { return Ok(()); }
                        }
                        _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                    }
                }
                Err(error) => return Err(error),
            }
        }
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
                    match (dependencies.take_work)().await {
                        Ok(()) => {}
                        Err(WorkerError::DependencyUnavailable | WorkerError::JobStateUnavailable) => continue 'reconnect,
                        Err(error) => return Err(error),
                    }
                }
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
    let correlation = Correlation::for_job(lease.job_id).with_attempt(lease.attempt);
    let Some(permit) = gate.reserve(lease, required_budget).await.map_err(|_| {
        MetricEvent::count(MetricName::Errors, correlation, 1, None)
            .with_result_class("job_state_unavailable")
            .emit();
        WorkerError::JobStateUnavailable
    })?
    else {
        return Ok(None);
    };
    let started = Instant::now();
    let task = operation();
    permit.commit().await.map_err(|_| {
        MetricEvent::count(MetricName::Errors, correlation, 1, None)
            .with_result_class("job_state_unavailable")
            .emit();
        WorkerError::JobStateUnavailable
    })?;
    MetricEvent::count(MetricName::ProviderCalls, correlation, 1, None).emit();
    let result = task.await;
    MetricEvent::duration(
        MetricName::ProviderLatency,
        correlation,
        started.elapsed(),
        None,
    )
    .with_result_class(if result.is_ok() { "ok" } else { "failed" })
    .emit();
    result.map(Some).map_err(|_| {
        MetricEvent::count(MetricName::ProviderFailures, correlation, 1, None)
            .with_result_class("provider_task_failed")
            .emit();
        WorkerError::ProviderTaskFailed
    })
}
