use gpsart_routing::{BenchmarkHarness, BenchmarkOutcome, BenchmarkPins, QualityBand};
use std::time::Duration;

fn pins(template_id: &str) -> BenchmarkPins {
    BenchmarkPins {
        map_version: "cubbon-park-osm-v1+c4a009be".into(),
        profile_version: "walking-v1".into(),
        template_id: template_id.into(),
        template_version: 1,
        template_hash: "immutable-template-hash".into(),
        algorithm_version: "candidate-quality-v1".into(),
    }
}

#[test]
fn harness_records_pinned_inputs_runtime_calls_outcome_validity_and_artifacts() {
    let mut harness = BenchmarkHarness::new("gpsart-routing-benchmark-v1");
    harness
        .record(
            "pilot-heart-001",
            pins("heart"),
            Duration::from_millis(14),
            3,
            BenchmarkOutcome::Match,
            true,
            Some(QualityBand::StrongMatch),
            Some("docs/evidence/ts-107-pilot-heart.svg".into()),
        )
        .unwrap();
    harness
        .record(
            "pilot-hi-001",
            pins("HI"),
            Duration::from_millis(20),
            5,
            BenchmarkOutcome::NoMatch,
            true,
            None,
            None,
        )
        .unwrap();

    let report = harness.finish().unwrap();
    assert_eq!(report.cases.len(), 2);
    assert_eq!(report.cases[0].latency_millis, 14);
    assert_eq!(report.cases[0].provider_calls, 3);
    assert_eq!(
        report.cases[0].route_artifact.as_deref(),
        Some("docs/evidence/ts-107-pilot-heart.svg")
    );
    let json = serde_json::to_string_pretty(&report).unwrap();
    assert!(json.contains("\"algorithm_version\": \"candidate-quality-v1\""));
    assert!(!json.contains("latitude"));
    assert!(!json.contains("longitude"));
}

#[test]
fn harness_rejects_unpinned_or_inconsistent_results() {
    let mut harness = BenchmarkHarness::new("gpsart-routing-benchmark-v1");
    let mut missing_map = pins("heart");
    missing_map.map_version.clear();
    assert!(
        harness
            .record(
                "bad",
                missing_map,
                Duration::ZERO,
                0,
                BenchmarkOutcome::Match,
                true,
                Some(QualityBand::StrongMatch),
                None,
            )
            .is_err()
    );
    assert!(
        harness
            .record(
                "false-success",
                pins("heart"),
                Duration::ZERO,
                0,
                BenchmarkOutcome::NoMatch,
                true,
                Some(QualityBand::StrongMatch),
                None,
            )
            .is_err()
    );
}
