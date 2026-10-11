use gpsart_observability::{
    Correlation, DesignCategory, DistanceBucket, MetricEvent, MetricName, RouteDimensions,
};
use std::time::Duration;
use uuid::Uuid;

#[test]
fn buckets_distance_without_retaining_an_exact_value() {
    assert_eq!(DistanceBucket::from_metres(999), DistanceBucket::UnderOneKm);
    assert_eq!(
        DistanceBucket::from_metres(1_000),
        DistanceBucket::OneToFiveKm
    );
    assert_eq!(
        DistanceBucket::from_metres(100_000),
        DistanceBucket::FiftyToOneHundredKm
    );
    assert_eq!(
        DistanceBucket::from_metres(100_001),
        DistanceBucket::OverOneHundredKm
    );
}

#[test]
fn one_opaque_correlation_connects_every_stage_with_only_coarse_dimensions() {
    let job_id = Uuid::new_v4();
    let correlation = Correlation::for_job(job_id);
    let dimensions = RouteDimensions::new(
        "walking",
        DistanceBucket::OneToFiveKm,
        DesignCategory::Shape,
        "walking-v1",
        "1",
        "pilot-map-v1",
        "algorithm-v1",
    )
    .unwrap();

    let acknowledgement = MetricEvent::duration(
        MetricName::AcknowledgementLatency,
        correlation,
        Duration::from_millis(12),
        Some(dimensions.clone()),
    );
    let provider = MetricEvent::count(
        MetricName::ProviderCalls,
        correlation.with_attempt(2).with_provider_call(4),
        1,
        Some(dimensions),
    );

    assert_eq!(acknowledgement.correlation.correlation_id, job_id);
    assert_eq!(provider.correlation.correlation_id, job_id);
    assert_eq!(provider.correlation.attempt, Some(2));
    assert_eq!(provider.correlation.provider_call, Some(4));
    let encoded = serde_json::to_string(&provider).unwrap();
    assert!(!encoded.contains("latitude"));
    assert!(!encoded.contains("longitude"));
    assert!(!encoded.contains("geometry"));
}

#[test]
fn versions_are_bounded_machine_identifiers_not_custom_text() {
    assert!(
        RouteDimensions::new(
            "walking",
            DistanceBucket::OneToFiveKm,
            DesignCategory::Text,
            "walking-v1",
            "1",
            "pilot-map-v1",
            "algorithm-v1",
        )
        .is_ok()
    );
    assert!(
        RouteDimensions::new(
            "walking",
            DistanceBucket::OneToFiveKm,
            DesignCategory::Text,
            "walking-v1",
            "private free-form text",
            "pilot-map-v1",
            "algorithm-v1",
        )
        .is_err()
    );
}

#[test]
fn representative_coordinate_payloads_are_redacted_before_the_logging_sink() {
    let correlation = Correlation::for_job(Uuid::new_v4());
    let event = MetricEvent::count(MetricName::Errors, correlation, 1, None)
        .with_result_class("51.5074,-0.1278");
    let encoded = serde_json::to_string(&event).unwrap();
    assert!(!encoded.contains("51.5074"));
    assert!(!encoded.contains("-0.1278"));
    assert!(encoded.contains("redacted_invalid_result_class"));
}
