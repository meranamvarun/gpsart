use gpsart_persistence::jobs::{JobError, JobLease};
use gpsart_worker::{ProviderCallPermit, ProviderGate, run_provider_call};
use gpsart_worker::{WorkerDependencies, run_until_shutdown};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, SystemTime};
use tokio::sync::watch;
use uuid::Uuid;

#[tokio::test]
async fn worker_checks_dependencies_and_stops_taking_work_after_shutdown() {
    let checks = Arc::new(AtomicUsize::new(0));
    let leases = Arc::new(AtomicUsize::new(0));
    let dependencies = WorkerDependencies::for_test(checks.clone(), leases.clone());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    shutdown_tx.send(true).unwrap();
    run_until_shutdown(dependencies, shutdown_rx).await.unwrap();
    assert_eq!(checks.load(Ordering::SeqCst), 1);
    assert_eq!(leases.load(Ordering::SeqCst), 0);
}

struct Gate(bool);

struct Permit<'a>(&'a AtomicUsize);

#[async_trait::async_trait]
impl ProviderCallPermit for Permit<'_> {
    async fn commit(self: Box<Self>) -> Result<(), JobError> {
        self.0.store(2, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait::async_trait]
impl ProviderGate for Gate {
    async fn reserve(
        &self,
        _: &JobLease,
        _: Duration,
    ) -> Result<Option<Box<dyn ProviderCallPermit>>, JobError> {
        Ok(self
            .0
            .then(|| Box::new(Permit(&CALL_ORDER)) as Box<dyn ProviderCallPermit>))
    }
}

static CALL_ORDER: AtomicUsize = AtomicUsize::new(0);

#[tokio::test]
async fn provider_call_is_not_scheduled_after_cancel_or_insufficient_budget() {
    let lease = JobLease {
        job_id: Uuid::new_v4(),
        worker_id: Uuid::new_v4(),
        attempt: 1,
        lease_expires_at: SystemTime::now() + Duration::from_secs(30),
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let provider_calls = calls.clone();
    let result = run_provider_call(&Gate(false), &lease, Duration::from_secs(5), || {
        tokio::spawn(async move { provider_calls.fetch_add(1, Ordering::SeqCst) })
    })
    .await
    .unwrap();
    assert!(result.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn provider_operation_is_scheduled_before_the_atomic_permit_is_released() {
    CALL_ORDER.store(0, Ordering::SeqCst);
    let lease = JobLease {
        job_id: Uuid::new_v4(),
        worker_id: Uuid::new_v4(),
        attempt: 1,
        lease_expires_at: SystemTime::now() + Duration::from_secs(30),
    };

    let result = run_provider_call(&Gate(true), &lease, Duration::from_secs(5), || {
        assert_eq!(CALL_ORDER.load(Ordering::SeqCst), 0);
        CALL_ORDER.store(1, Ordering::SeqCst);
        tokio::spawn(async { "scheduled" })
    })
    .await
    .unwrap();

    assert_eq!(result, Some("scheduled"));
    assert_eq!(CALL_ORDER.load(Ordering::SeqCst), 2);
}
