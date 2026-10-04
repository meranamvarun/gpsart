use async_trait::async_trait;
use gpsart_domain::{
    ActivityProfile, Metres, RouteRequestProfile,
    templates::{VersionedTemplate, embedded_catalogue},
};
use gpsart_routing::{
    AnchorMatch, AttemptControl, BenchmarkHarness, BenchmarkPins, CandidateQualityConfig,
    CandidateSearch, GeoPointE7, HeartCandidate, HeartSearchConfig, HeartSearchRequest,
    LocalOsmGraph, MatchOutcome, QUALITY_VERSION, RoutePath, RouteProvider, RouteQuery,
    RoutingError, score_candidate,
};
use std::fmt::Write as _;
use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const SNAPSHOT: &[u8] = include_bytes!("../data/cubbon-park-osm-v1.json");
const SNAPSHOT_HASH: &str = include_str!("../data/cubbon-park-osm-v1.sha256");

struct CountedGraph<'a> {
    graph: &'a LocalOsmGraph,
    provider_calls: AtomicU32,
}

impl AnchorMatch for CountedGraph<'_> {
    fn nearest_anchor(&self, target: GeoPointE7, tolerance_metres: u32) -> Option<GeoPointE7> {
        self.graph.nearest_anchor(target, tolerance_metres)
    }
}

#[async_trait]
impl RouteProvider for CountedGraph<'_> {
    async fn route(
        &self,
        query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        self.provider_calls.fetch_add(1, Ordering::Relaxed);
        self.graph.route(query, control).await
    }
}

fn request(graph: &LocalOsmGraph) -> HeartSearchRequest {
    HeartSearchRequest {
        profile: RouteRequestProfile::new(ActivityProfile::Walking),
        confirmed_start: graph.point(663_941_113).expect("pinned start node"),
        minimum_distance: Metres::new(1).unwrap(),
        maximum_distance: Metres::new(100_000).unwrap(),
    }
}

fn heart_config() -> HeartSearchConfig {
    HeartSearchConfig {
        placement_offsets_metres: (-3..=3)
            .flat_map(|east| (-3..=3).map(move |north| (east * 5, north * 5)))
            .collect(),
        scales_metres: vec![15, 20, 25, 30],
        rotations_degrees: (-12..=12).map(|step| step * 15).collect(),
        max_placements: 49,
        max_scales: 4,
        max_rotations: 25,
        max_anchor_matches: 100_000,
        max_provider_calls: 10_000,
        max_retained_points: 10_000,
        wall_time: Duration::from_secs(30),
        anchor_tolerance_metres: 5,
    }
}

fn text_config() -> HeartSearchConfig {
    let placement_offsets_metres = (-15..=0)
        .flat_map(|east| (-15..=5).map(move |north| (east, north)))
        .collect::<Vec<_>>();
    let scales_metres = (15..=30).collect::<Vec<_>>();
    let rotations_degrees = (-15..=15).collect::<Vec<_>>();
    HeartSearchConfig {
        max_placements: placement_offsets_metres.len(),
        max_scales: scales_metres.len(),
        max_rotations: rotations_degrees.len(),
        placement_offsets_metres,
        scales_metres,
        rotations_degrees,
        max_anchor_matches: 1_000_000,
        max_provider_calls: 10_000,
        max_retained_points: 10_000,
        wall_time: Duration::from_secs(30),
        anchor_tolerance_metres: 4,
    }
}

fn pins(template: &VersionedTemplate) -> BenchmarkPins {
    BenchmarkPins {
        map_version: format!("cubbon-park-osm-v1+{}", SNAPSHOT_HASH.trim()),
        profile_version: "walking-v1".into(),
        template_id: template.definition.id.clone(),
        template_version: template.definition.version,
        template_hash: template.definition_hash.clone(),
        algorithm_version: QUALITY_VERSION.into(),
    }
}

fn render_svg(candidate: &HeartCandidate, title: &str) -> String {
    let min_lat = candidate
        .geometry
        .iter()
        .map(|point| point.latitude_e7)
        .min()
        .unwrap();
    let max_lat = candidate
        .geometry
        .iter()
        .map(|point| point.latitude_e7)
        .max()
        .unwrap();
    let min_lon = candidate
        .geometry
        .iter()
        .map(|point| point.longitude_e7)
        .min()
        .unwrap();
    let max_lon = candidate
        .geometry
        .iter()
        .map(|point| point.longitude_e7)
        .max()
        .unwrap();
    let lat_span = f64::from((max_lat - min_lat).max(1));
    let lon_span = f64::from((max_lon - min_lon).max(1));
    let mut points = String::new();
    for point in &candidate.geometry {
        let x = 40.0 + f64::from(point.longitude_e7 - min_lon) / lon_span * 640.0;
        let y = 40.0 + f64::from(max_lat - point.latitude_e7) / lat_span * 400.0;
        write!(points, "{x:.1},{y:.1} ").unwrap();
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"720\" height=\"500\" viewBox=\"0 0 720 500\"><rect width=\"720\" height=\"500\" fill=\"#f7f4ee\"/><text x=\"40\" y=\"26\" font-family=\"sans-serif\" font-size=\"16\">{title} - canonical full trace</text><polyline points=\"{points}\" fill=\"none\" stroke=\"#d43f3a\" stroke-width=\"4\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/><circle cx=\"{start_x:.1}\" cy=\"{start_y:.1}\" r=\"7\" fill=\"#176b87\"/><text x=\"40\" y=\"480\" font-family=\"sans-serif\" font-size=\"13\">Pinned public pilot snapshot; coordinates normalized and omitted.</text></svg>\n",
        start_x = 40.0 + f64::from(candidate.geometry[0].longitude_e7 - min_lon) / lon_span * 640.0,
        start_y = 40.0 + f64::from(max_lat - candidate.geometry[0].latitude_e7) / lat_span * 400.0,
    )
}

#[tokio::main]
async fn main() {
    let graph = LocalOsmGraph::from_json(SNAPSHOT, SNAPSHOT_HASH.trim()).unwrap();
    let request = request(&graph);
    let catalogue = embedded_catalogue().unwrap();
    let mut harness = BenchmarkHarness::new("gpsart-routing-benchmark-v1");

    for (case_id, template_id, config, artifact) in [
        (
            "pilot-heart-001",
            "heart",
            heart_config(),
            "docs/evidence/ts-107-pilot-heart.svg",
        ),
        (
            "pilot-hi-001",
            "HI",
            text_config(),
            "docs/evidence/ts-107-pilot-hi.svg",
        ),
    ] {
        let template = catalogue.get(template_id, 1).unwrap().clone();
        let counted = CountedGraph {
            graph: &graph,
            provider_calls: AtomicU32::new(0),
        };
        let started = Instant::now();
        let outcome = if template_id == "HI" {
            CandidateSearch::new(&counted, &counted)
                .curated_text(&template, &request, &config)
                .await
        } else {
            CandidateSearch::new(&counted, &counted)
                .heart(&template, &request, &config)
                .await
        }
        .unwrap();
        let elapsed = started.elapsed();
        let MatchOutcome::Match(candidate) = outcome else {
            panic!("pinned pilot case {case_id} no longer matches")
        };
        let quality = score_candidate(
            *candidate,
            &template,
            &CandidateQualityConfig::v1(
                Metres::new(400).unwrap(),
                request.minimum_distance,
                request.maximum_distance,
            ),
        )
        .unwrap();
        fs::write(
            artifact,
            render_svg(&quality.candidate, &template.definition.name),
        )
        .unwrap();
        harness
            .record_scored_match(
                case_id,
                pins(&template),
                elapsed,
                counted.provider_calls.load(Ordering::Relaxed),
                &quality,
                artifact.into(),
            )
            .unwrap();
    }

    let report = harness.finish().unwrap();
    fs::write(
        "docs/evidence/ts-107-benchmark-results.json",
        format!("{}\n", serde_json::to_string_pretty(&report).unwrap()),
    )
    .unwrap();
}
