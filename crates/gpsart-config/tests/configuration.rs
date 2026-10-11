use gpsart_config::{AppConfig, ConfigError, DatabaseConfig, WorkloadLimits};

fn valid() -> Vec<(String, String)> {
    vec![
        (
            "GPSART_DATABASE_URL".into(),
            "postgres://app:secret@db/gpsart".into(),
        ),
        ("GPSART_API_BIND".into(), "127.0.0.1:8080".into()),
    ]
}

#[test]
fn worker_database_configuration_does_not_require_an_api_bind_address() {
    let config =
        DatabaseConfig::from_pairs([("GPSART_DATABASE_URL", "postgres://app:secret@db/gpsart")])
            .expect("worker config");
    assert!(!format!("{config:?}").contains("secret"));
}

#[test]
fn validates_required_environment_configuration() {
    let config = AppConfig::from_pairs(valid()).expect("valid config");
    assert_eq!(config.api_bind.to_string(), "127.0.0.1:8080");
    assert!(!format!("{config:?}").contains("secret"));
}

#[test]
fn rejects_missing_or_invalid_configuration_without_echoing_secrets() {
    assert_eq!(
        AppConfig::from_pairs(Vec::<(String, String)>::new()),
        Err(ConfigError::Missing("GPSART_DATABASE_URL"))
    );
    let error = AppConfig::from_pairs([
        ("GPSART_DATABASE_URL", "not-postgres://user:secret@host/db"),
        ("GPSART_API_BIND", "bad-address"),
    ]);
    assert!(!format!("{error:?}").contains("secret"));
}

#[test]
fn workload_defaults_cap_every_expensive_dimension() {
    let limits = WorkloadLimits::default();
    assert_eq!(limits.allowed_tolerance_percent, [5, 10, 20]);
    assert_eq!(limits.max_distance_metres, 100_000);
    assert!(limits.max_template_points <= 512);
    assert!(limits.max_anchors > 0);
    assert!(limits.max_candidate_attempts > 0);
    assert!(limits.max_provider_requests > 0);
    assert!(limits.max_response_bytes > 0);
    assert!(limits.max_runtime.as_secs() <= 60);
    assert!(limits.max_active_jobs_per_guest < limits.max_active_jobs_deployment);
    assert_eq!(limits.guest_retention.as_secs(), 86_400);
}

#[test]
fn workload_limits_are_configurable_but_cannot_weaken_release_caps() {
    let mut pairs = valid();
    pairs.push(("GPSART_MAX_ACTIVE_JOBS_PER_GUEST".into(), "1".into()));
    pairs.push(("GPSART_MAX_ACTIVE_JOBS_DEPLOYMENT".into(), "4".into()));
    pairs.push(("GPSART_GUEST_RETENTION_SECONDS".into(), "3600".into()));
    let config = AppConfig::from_pairs(pairs).unwrap();
    assert_eq!(config.workload_limits.max_active_jobs_per_guest, 1);
    assert_eq!(config.workload_limits.guest_retention.as_secs(), 3600);

    let mut weakened = valid();
    weakened.push(("GPSART_GUEST_RETENTION_SECONDS".into(), "86401".into()));
    assert_eq!(
        AppConfig::from_pairs(weakened),
        Err(ConfigError::Invalid("GPSART_WORKLOAD_LIMITS"))
    );
}
