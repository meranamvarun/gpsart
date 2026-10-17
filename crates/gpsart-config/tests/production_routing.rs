use gpsart_config::{ProductionRoutingConfig, ProductionRoutingError, WorkloadLimits};
use gpsart_domain::starts::{Point, StartResolverProvider};
use gpsart_domain::{ActivityProfile, ProfileVersion, RouteRequestProfile};
use gpsart_routing::LocalOsmGraph;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ops/routing/production-pilot-v1.json"
);
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn temporary_manifest(mut change: impl FnMut(&mut Value)) -> PathBuf {
    let source = Path::new(MANIFEST);
    let mut value: Value = serde_json::from_slice(&fs::read(source).unwrap()).unwrap();
    change(&mut value);
    let directory = std::env::temp_dir().join(format!(
        "gpsart-ts503-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&directory).unwrap();
    let graph_source = source
        .parent()
        .unwrap()
        .join(value["graph"]["path"].as_str().unwrap());
    let graph_target = directory.join("graph.json");
    fs::copy(graph_source.canonicalize().unwrap(), &graph_target).unwrap();
    let script_source = source
        .parent()
        .unwrap()
        .join(value["graph"]["extraction_script_path"].as_str().unwrap());
    fs::copy(
        script_source.canonicalize().unwrap(),
        directory.join("extract.mjs"),
    )
    .unwrap();
    value["graph"]["path"] = Value::String("graph.json".into());
    value["graph"]["extraction_script_path"] = Value::String("extract.mjs".into());
    let path = directory.join("manifest.json");
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    path
}

#[test]
fn approved_manifest_pins_graph_starts_coverage_and_reviewed_limits() {
    let config = ProductionRoutingConfig::load(MANIFEST).expect("approved manifest");
    assert_eq!(
        config.map_data_version(),
        "osm-bengaluru-central-pedestrian-pilot-v3:sha256:df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45"
    );
    assert_eq!(
        config.coverage().version,
        "bengaluru-central-pilot-coverage-v1"
    );
    assert!(
        config
            .coverage()
            .contains(Point::new(77.578203, 12.9652039).unwrap())
    );
    assert!(
        config
            .coverage()
            .contains(Point::new(77.607869, 12.9883305).unwrap())
    );
    assert!(
        !config
            .coverage()
            .contains(Point::new(77.5782029, 12.9652039).unwrap())
    );
    config
        .validate_workload(&WorkloadLimits::default())
        .unwrap();

    let provider = config.network_provider().unwrap();
    let start = config.reviewed_starts()[0].point;
    for activity in [ActivityProfile::Walking, ActivityProfile::Running] {
        assert!(
            provider
                .nearest_routable(start, RouteRequestProfile::new(activity))
                .unwrap()
                .is_some()
        );
    }
    assert_eq!(
        provider
            .nearest_routable(start, RouteRequestProfile::new(ActivityProfile::Cycling))
            .unwrap(),
        None,
        "an unreviewed activity must not inherit a reviewed start"
    );
}

#[test]
fn startup_rejects_missing_malformed_mismatched_empty_and_unknown_inputs() {
    assert!(matches!(
        ProductionRoutingConfig::load("definitely/missing.json"),
        Err(ProductionRoutingError::ManifestUnavailable)
    ));

    let cases = [
        (
            "unknown schema",
            Box::new(|v: &mut Value| v["schema_version"] = Value::String("future-v9".into()))
                as Box<dyn Fn(&mut Value)>,
        ),
        (
            "checksum mismatch",
            Box::new(|v: &mut Value| v["graph"]["sha256"] = Value::String("0".repeat(64))),
        ),
        (
            "map version mismatch",
            Box::new(|v: &mut Value| {
                v["map_data_version"] = Value::String("other:sha256:deadbeef".into())
            }),
        ),
        (
            "empty starts",
            Box::new(|v: &mut Value| v["routable_starts"] = Value::Array(vec![])),
        ),
        (
            "unknown profile",
            Box::new(|v: &mut Value| {
                v["routable_starts"][0]["profile_versions"] = serde_json::json!(["hoverboard-v1"])
            }),
        ),
        (
            "coordinate mismatch",
            Box::new(|v: &mut Value| {
                v["routable_starts"][0]["point"]["latitude"] = serde_json::json!(0.0)
            }),
        ),
        (
            "provenance mismatch",
            Box::new(|v: &mut Value| {
                v["graph"]["source_raw_sha256"] = Value::String("a".repeat(64))
            }),
        ),
        (
            "coverage excludes graph",
            Box::new(|v: &mut Value| v["coverage"]["bounds"][2] = serde_json::json!(77.59)),
        ),
        (
            "extractor mismatch",
            Box::new(|v: &mut Value| {
                v["graph"]["extraction_script_sha256"] = Value::String("a".repeat(64))
            }),
        ),
        (
            "unreviewed approval",
            Box::new(|v: &mut Value| v["approval"]["status"] = Value::String("draft".into())),
        ),
    ];
    for (name, change) in cases {
        assert!(
            ProductionRoutingConfig::load(temporary_manifest(change)).is_err(),
            "accepted {name}"
        );
    }
}

#[test]
fn workload_must_match_the_reviewed_canonical_metre_configuration() {
    let config = ProductionRoutingConfig::load(MANIFEST).unwrap();
    let mut changed = WorkloadLimits::default();
    changed.max_distance_metres -= 1;
    assert_eq!(
        config.validate_workload(&changed),
        Err(ProductionRoutingError::WorkloadMismatch)
    );
    assert!(config.reviewed_starts().iter().all(|start| {
        !start.profile_versions.is_empty()
            && start.profile_versions.iter().all(|profile| {
                matches!(
                    profile,
                    ProfileVersion::WalkingV1 | ProfileVersion::RunningV1
                )
            })
    }));
}

#[test]
fn historical_graph_snapshots_remain_checksum_addressable() {
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../gpsart-routing/data");
    let mut versions = Vec::new();
    for name in [
        "cubbon-park-osm-v1",
        "cubbon-park-osm-v2",
        "bengaluru-central-osm-v3",
    ] {
        let bytes = fs::read(data.join(format!("{name}.json"))).unwrap();
        let checksum = fs::read_to_string(data.join(format!("{name}.sha256"))).unwrap();
        let graph = LocalOsmGraph::from_json(&bytes, checksum.trim()).expect("historical graph");
        versions.push(graph.data_version().to_owned());
    }
    versions.sort();
    versions.dedup();
    assert_eq!(
        versions.len(),
        3,
        "an update must not relabel old graph bytes"
    );
}
