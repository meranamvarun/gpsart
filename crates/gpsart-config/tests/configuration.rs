use gpsart_config::{AppConfig, ConfigError, DatabaseConfig};

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
