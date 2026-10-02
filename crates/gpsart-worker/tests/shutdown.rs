use gpsart_worker::{WorkerDependencies, run_until_shutdown};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::watch;

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
