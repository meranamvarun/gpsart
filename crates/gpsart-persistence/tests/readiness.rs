use gpsart_persistence::{DependencyProbe, PostgresProbe};
use std::time::Duration;

#[tokio::test]
async fn unavailable_postgres_fails_readiness_without_leaking_the_url() {
    let probe = PostgresProbe::new(
        "postgres://private:do-not-log@127.0.0.1:1/gpsart".into(),
        Duration::from_millis(100),
    );
    let error = probe.check().await.expect_err("port is unavailable");
    assert_eq!(error.code(), "postgres_unavailable");
    assert!(!format!("{error:?}").contains("do-not-log"));
}
