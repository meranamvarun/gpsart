use gpsart_domain::{ActivityProfile, RouteRequestProfile};
use gpsart_routing::{
    AttemptControl, GeoPointE7, LocalOsmGraph, Qualification, RouteProvider, RouteQuery,
    RoutingError,
};
use std::time::Duration;

const PILOT: &[u8] = include_bytes!("../data/cubbon-park-osm-v1.json");
const PILOT_SHA256: &str = include_str!("../data/cubbon-park-osm-v1.sha256");

fn control() -> AttemptControl {
    AttemptControl::with_timeout(Duration::from_secs(1), 1)
}

fn query(activity: ActivityProfile, anchors: &[&str]) -> RouteQuery {
    RouteQuery::new(RouteRequestProfile::new(activity), anchors.iter().copied())
}

fn inline_snapshot(tags: &str) -> Vec<u8> {
    format!(
        r#"{{
  "schema_version": 1,
  "snapshot_id": "test-snapshot-v1",
  "source": {{
    "api": "OpenStreetMap API 0.6",
    "url": "https://api.openstreetmap.org/api/0.6/map.json?bbox=0,0,1,1",
    "retrieved_at": "2026-10-04T00:00:00Z",
    "raw_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "attribution": "OpenStreetMap contributors",
    "license": "ODbL-1.0"
  }},
  "nodes": [
    {{"id": 1, "version": 1, "timestamp": "2026-01-01T00:00:00Z", "latitude_e7": 100000000, "longitude_e7": 770000000}},
    {{"id": 2, "version": 1, "timestamp": "2026-01-01T00:00:00Z", "latitude_e7": 100000100, "longitude_e7": 770000000}},
    {{"id": 3, "version": 1, "timestamp": "2026-01-01T00:00:00Z", "latitude_e7": 100000200, "longitude_e7": 770000000}}
  ],
  "ways": [{{
    "id": 10,
    "version": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "nodes": [1, 2, 3],
    "tags": {tags}
  }}],
  "forbidden_turns": []
}}"#
    )
    .into_bytes()
}

#[test]
fn rejects_checksum_mismatch_before_parsing() {
    assert!(matches!(
        LocalOsmGraph::from_json(b"not json", &"00".repeat(32)),
        Err(RoutingError::SnapshotChecksumMismatch)
    ));
}

#[test]
fn rejects_malformed_or_unversioned_snapshot_elements() {
    let bytes = inline_snapshot(
        r#"{"highway":"unclassified","foot":"yes","bicycle":"yes","oneway":"yes"}"#,
    );
    let malformed = String::from_utf8(bytes)
        .unwrap()
        .replace("\"version\": 1", "\"version\": 0");
    let checksum = LocalOsmGraph::sha256_hex(malformed.as_bytes());
    assert!(matches!(
        LocalOsmGraph::from_json(malformed.as_bytes(), &checksum),
        Err(RoutingError::InvalidSnapshot)
    ));
}

#[tokio::test]
async fn applies_profile_specific_osm_defaults_without_guessing_unknown_access() {
    let missing_direction =
        inline_snapshot(r#"{"highway":"unclassified","foot":"yes","bicycle":"yes"}"#);
    let graph = LocalOsmGraph::from_json(
        &missing_direction,
        &LocalOsmGraph::sha256_hex(&missing_direction),
    )
    .unwrap();
    assert!(
        graph
            .route(
                &query(ActivityProfile::Walking, &["osm-node/3", "osm-node/1"]),
                &control(),
            )
            .await
            .is_ok()
    );

    let foot_only =
        inline_snapshot(r#"{"highway":"unclassified","foot":"yes","bicycle":"no","oneway":"yes"}"#);
    let graph =
        LocalOsmGraph::from_json(&foot_only, &LocalOsmGraph::sha256_hex(&foot_only)).unwrap();
    assert!(
        graph
            .route(
                &query(ActivityProfile::Walking, &["osm-node/3", "osm-node/1"]),
                &control(),
            )
            .await
            .is_ok()
    );
    assert_eq!(
        graph
            .route(
                &query(ActivityProfile::Cycling, &["osm-node/1", "osm-node/3"]),
                &control(),
            )
            .await,
        Err(RoutingError::NoPath)
    );

    let unknown_access =
        inline_snapshot(r#"{"highway":"unclassified","oneway":"yes","surface":"asphalt"}"#);
    let graph =
        LocalOsmGraph::from_json(&unknown_access, &LocalOsmGraph::sha256_hex(&unknown_access))
            .unwrap();
    let route = graph
        .route(
            &query(ActivityProfile::Walking, &["osm-node/1", "osm-node/3"]),
            &control(),
        )
        .await
        .unwrap();
    assert!(
        route
            .edges
            .iter()
            .all(|edge| edge.qualifications.contains(&Qualification::AccessUnknown))
    );

    for (explicit_denial, denied_profile, unknown_profile) in [
        (
            "\"bicycle\":\"no\",",
            ActivityProfile::Cycling,
            ActivityProfile::Walking,
        ),
        (
            "\"foot\":\"no\",",
            ActivityProfile::Walking,
            ActivityProfile::Cycling,
        ),
    ] {
        let tags = format!("{{\"highway\":\"unclassified\",{explicit_denial}\"oneway\":\"yes\"}}");
        let bytes = inline_snapshot(&tags);
        let graph = LocalOsmGraph::from_json(&bytes, &LocalOsmGraph::sha256_hex(&bytes)).unwrap();
        assert_eq!(
            graph
                .route(
                    &query(denied_profile, &["osm-node/1", "osm-node/3"]),
                    &control(),
                )
                .await,
            Err(RoutingError::NoPath)
        );
        let route = graph
            .route(
                &query(unknown_profile, &["osm-node/1", "osm-node/3"]),
                &control(),
            )
            .await
            .unwrap();
        assert!(
            route
                .edges
                .iter()
                .all(|edge| edge.qualifications.contains(&Qualification::AccessUnknown))
        );
    }

    for (conditional_tag, affected_profile, unaffected_profile) in [
        (
            "\"foot:conditional\":\"no @ (sunset-sunrise)\",",
            ActivityProfile::Walking,
            ActivityProfile::Cycling,
        ),
        (
            "\"bicycle:conditional\":\"no @ (sunset-sunrise)\",",
            ActivityProfile::Cycling,
            ActivityProfile::Walking,
        ),
    ] {
        let tags = format!(
            "{{\"highway\":\"unclassified\",\"foot\":\"yes\",\"bicycle\":\"yes\",{conditional_tag}\"oneway\":\"yes\"}}"
        );
        let bytes = inline_snapshot(&tags);
        let graph = LocalOsmGraph::from_json(&bytes, &LocalOsmGraph::sha256_hex(&bytes)).unwrap();
        let affected = graph
            .route(
                &query(affected_profile, &["osm-node/1", "osm-node/3"]),
                &control(),
            )
            .await
            .unwrap();
        assert!(
            affected
                .edges
                .iter()
                .all(|edge| edge.qualifications.contains(&Qualification::AccessUnknown))
        );
        let unaffected = graph
            .route(
                &query(unaffected_profile, &["osm-node/1", "osm-node/3"]),
                &control(),
            )
            .await
            .unwrap();
        assert!(
            unaffected
                .edges
                .iter()
                .all(|edge| !edge.qualifications.contains(&Qualification::AccessUnknown))
        );
    }

    let general_conditional = inline_snapshot(
        r#"{"highway":"unclassified","access":"yes","access:conditional":"no @ (sunset-sunrise)","oneway":"yes"}"#,
    );
    let graph = LocalOsmGraph::from_json(
        &general_conditional,
        &LocalOsmGraph::sha256_hex(&general_conditional),
    )
    .unwrap();
    for profile in [ActivityProfile::Walking, ActivityProfile::Cycling] {
        let route = graph
            .route(&query(profile, &["osm-node/1", "osm-node/3"]), &control())
            .await
            .unwrap();
        assert!(
            route
                .edges
                .iter()
                .all(|edge| edge.qualifications.contains(&Qualification::AccessUnknown))
        );
    }

    let explicit_profile_denial = inline_snapshot(
        r#"{"highway":"unclassified","access":"yes","access:conditional":"no @ (sunset-sunrise)","bicycle":"no","oneway":"yes"}"#,
    );
    let graph = LocalOsmGraph::from_json(
        &explicit_profile_denial,
        &LocalOsmGraph::sha256_hex(&explicit_profile_denial),
    )
    .unwrap();
    assert_eq!(
        graph
            .route(
                &query(ActivityProfile::Cycling, &["osm-node/1", "osm-node/3"]),
                &control(),
            )
            .await,
        Err(RoutingError::NoPath)
    );
    let walking = graph
        .route(
            &query(ActivityProfile::Walking, &["osm-node/1", "osm-node/3"]),
            &control(),
        )
        .await
        .unwrap();
    assert!(
        walking
            .edges
            .iter()
            .all(|edge| edge.qualifications.contains(&Qualification::AccessUnknown))
    );
}

#[tokio::test]
async fn footway_defaults_are_walkable_but_explicit_foot_direction_is_enforced() {
    let implicit = inline_snapshot(r#"{"highway":"footway"}"#);
    let graph = LocalOsmGraph::from_json(&implicit, &LocalOsmGraph::sha256_hex(&implicit)).unwrap();
    let route = graph
        .route(
            &query(ActivityProfile::Walking, &["osm-node/3", "osm-node/1"]),
            &control(),
        )
        .await
        .unwrap();
    assert!(
        route
            .edges
            .iter()
            .all(|edge| { !edge.qualifications.contains(&Qualification::AccessUnknown) })
    );
    assert_eq!(route.legality.policy_version, "osm-pedestrian-policy-v2");
    assert!(route.legality.access_verified);
    assert!(route.legality.direction_verified);
    assert!(route.legality.turns_verified);

    let explicit = inline_snapshot(r#"{"highway":"footway","oneway:foot":"yes"}"#);
    let graph = LocalOsmGraph::from_json(&explicit, &LocalOsmGraph::sha256_hex(&explicit)).unwrap();
    assert!(
        graph
            .route(
                &query(ActivityProfile::Walking, &["osm-node/1", "osm-node/3"]),
                &control(),
            )
            .await
            .is_ok()
    );
    assert_eq!(
        graph
            .route(
                &query(ActivityProfile::Walking, &["osm-node/3", "osm-node/1"]),
                &control(),
            )
            .await,
        Err(RoutingError::NoPath)
    );
}

#[tokio::test]
async fn conditional_oneway_is_unknown_and_never_treated_as_unconditionally_bidirectional() {
    let bytes = inline_snapshot(
        r#"{"highway":"unclassified","foot":"yes","bicycle":"yes","oneway":"no","oneway:conditional":"yes @ (sunset-sunrise)"}"#,
    );
    let graph = LocalOsmGraph::from_json(&bytes, &LocalOsmGraph::sha256_hex(&bytes)).unwrap();
    assert_eq!(
        graph
            .route(
                &query(ActivityProfile::Cycling, &["osm-node/3", "osm-node/1"]),
                &control(),
            )
            .await,
        Err(RoutingError::NoPath)
    );

    let path_bytes =
        inline_snapshot(r#"{"highway":"footway","oneway:conditional":"yes @ (sunset-sunrise)"}"#);
    let graph =
        LocalOsmGraph::from_json(&path_bytes, &LocalOsmGraph::sha256_hex(&path_bytes)).unwrap();
    assert_eq!(
        graph
            .route(
                &query(ActivityProfile::Walking, &["osm-node/1", "osm-node/3"]),
                &control(),
            )
            .await,
        Err(RoutingError::NoPath)
    );
}

#[tokio::test]
async fn accepts_only_exact_snapshot_points_so_anchors_are_never_silently_moved() {
    let bytes = inline_snapshot(
        r#"{"highway":"unclassified","foot":"yes","bicycle":"yes","oneway":"yes"}"#,
    );
    let graph = LocalOsmGraph::from_json(&bytes, &LocalOsmGraph::sha256_hex(&bytes)).unwrap();
    let exact = RouteQuery::from_points(
        RouteRequestProfile::new(ActivityProfile::Walking),
        [
            GeoPointE7::new(100000000, 770000000),
            GeoPointE7::new(100000200, 770000000),
        ],
    );
    assert!(graph.route(&exact, &control()).await.is_ok());

    let moved = RouteQuery::from_points(
        RouteRequestProfile::new(ActivityProfile::Walking),
        [
            GeoPointE7::new(100000001, 770000000),
            GeoPointE7::new(100000200, 770000000),
        ],
    );
    assert_eq!(
        graph.route(&moved, &control()).await,
        Err(RoutingError::InvalidRequest)
    );
}

#[tokio::test]
async fn computes_short_dateline_segments_without_integer_overflow() {
    let bytes = String::from_utf8(inline_snapshot(
        r#"{"highway":"unclassified","foot":"yes","bicycle":"yes","oneway":"yes"}"#,
    ))
    .unwrap()
    .replacen("770000000", "1799999990", 1)
    .replacen("770000000", "-1799999990", 1)
    .replacen("770000000", "-1799999980", 1)
    .into_bytes();
    let graph = LocalOsmGraph::from_json(&bytes, &LocalOsmGraph::sha256_hex(&bytes)).unwrap();
    let route = graph
        .route(
            &query(ActivityProfile::Walking, &["osm-node/1", "osm-node/3"]),
            &control(),
        )
        .await
        .unwrap();
    assert!(route.distance.value() < 10);
}

#[tokio::test]
async fn pinned_bengaluru_snapshot_returns_auditable_walking_and_cycling_paths() {
    let graph = LocalOsmGraph::from_json(PILOT, PILOT_SHA256.trim()).unwrap();
    let anchors = ["osm-node/249088242", "osm-node/4017403934"];

    for activity in [ActivityProfile::Walking, ActivityProfile::Cycling] {
        let route = graph
            .route(&query(activity, &anchors), &control())
            .await
            .unwrap();
        assert_eq!(route.geometry.first(), graph.point(249088242).as_ref());
        assert_eq!(route.geometry.last(), graph.point(4017403934).as_ref());
        assert_eq!(
            route.edges.len(),
            match activity {
                ActivityProfile::Walking => 26,
                ActivityProfile::Cycling => 28,
                ActivityProfile::Running => unreachable!(),
            }
        );
        assert_eq!(
            route.edges.first().unwrap().edge_id,
            "osm-way/46728546/v27/segment/0"
        );
        assert_eq!(
            route.edges.last().unwrap().edge_id,
            "osm-way/52032471/v12/segment/4"
        );
        assert_eq!(
            route.distance.value(),
            route
                .edges
                .iter()
                .map(|edge| edge.distance.unwrap().value())
                .sum::<u32>()
        );
        assert_eq!(route.provenance.provider, "local-osm-snapshot");
        assert!(
            route
                .provenance
                .data_version
                .as_deref()
                .unwrap()
                .ends_with(PILOT_SHA256.trim())
        );
        assert_eq!(
            route.provenance.attribution.as_deref(),
            Some("OpenStreetMap contributors (ODbL 1.0)")
        );
        assert_eq!(route.legality.policy_version, "osm-pedestrian-policy-v2");
    }
}

#[tokio::test]
async fn repeated_pilot_travel_remains_repeated_edge_occurrences() {
    let graph = LocalOsmGraph::from_json(PILOT, PILOT_SHA256.trim()).unwrap();
    let route = graph
        .route(
            &query(
                ActivityProfile::Cycling,
                &[
                    "osm-node/663941113",
                    "osm-node/12190248444",
                    "osm-node/663941113",
                    "osm-node/12190248444",
                ],
            ),
            &control(),
        )
        .await
        .unwrap();
    let repeated = "osm-way/369295564/v13/segment/7";
    assert_eq!(
        route
            .edges
            .iter()
            .filter(|edge| edge.edge_id == repeated)
            .count(),
        2
    );
    assert_eq!(
        route.distance.value(),
        route
            .edges
            .iter()
            .map(|edge| edge.distance.unwrap().value())
            .sum::<u32>()
    );
}
