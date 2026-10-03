use gpsart_persistence::jobs::{JobError, JobLease};
use gpsart_worker::{ProviderGate, run_provider_call};
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

#[async_trait::async_trait]
impl ProviderGate for Gate {
    async fn may_schedule(&self, _: &JobLease, _: Duration) -> Result<bool, JobError> {
        Ok(self.0)
    }
}

#[tokio::test]
async fn provider_call_is_not_scheduled_after_cancel_or_insufficient_budget() {
    let lease = JobLease {
        job_id: Uuid::new_v4(),
        worker_id: Uuid::new_v4(),
        attempt: 1,
        lease_expires_at: SystemTime::now() + Duration::from_secs(30),
    };
    let calls = AtomicUsize::new(0);
    let result = run_provider_call(&Gate(false), &lease, Duration::from_secs(5), || {
        calls.fetch_add(1, Ordering::SeqCst)
    })
    .await
    .unwrap();
    assert!(result.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
