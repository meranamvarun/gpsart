use async_trait::async_trait;
use gpsart_domain::{
    ActivityProfile, Metres, RouteRequestProfile,
    templates::{VersionedTemplate, embedded_catalogue},
};
use gpsart_routing::{
    AnchorMatch, AttemptControl, BENCHMARK_SCHEMA_VERSION, BenchmarkHarness, BenchmarkPins,
    CandidatePart, CandidateQualityConfig, CandidateSearch, GeoPointE7, HeartCandidate,
    HeartSearchConfig, HeartSearchRequest, LocalOsmGraph, MatchOutcome, QUALITY_VERSION, RoutePath,
    RouteProvider, RouteQuery, RoutingError, score_candidate,
};
use std::fmt::Write as _;
use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const SNAPSHOT: &[u8] = include_bytes!("../data/cubbon-park-osm-v2.json");
const SNAPSHOT_HASH: &str = include_str!("../data/cubbon-park-osm-v2.sha256");

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

fn request(graph: &LocalOsmGraph, start_node_id: u64) -> HeartSearchRequest {
    HeartSearchRequest {
        profile: RouteRequestProfile::new(ActivityProfile::Walking),
        confirmed_start: graph.point(start_node_id).expect("pinned start node"),
        minimum_distance: Metres::new(1).unwrap(),
        maximum_distance: Metres::new(100_000).unwrap(),
    }
}

fn heart_config() -> HeartSearchConfig {
    HeartSearchConfig {
        placement_offsets_metres: (-149..=-143)
            .flat_map(|east| (79..=85).map(move |north| (east, north)))
            .collect(),
        scales_metres: vec![400],
        rotations_degrees: vec![60],
        max_placements: 49,
        max_scales: 1,
        max_rotations: 1,
        max_anchor_matches: 1_000,
        max_provider_calls: 49,
        max_retained_points: 10_000,
        wall_time: Duration::from_secs(5),
        anchor_tolerance_metres: 15,
    }
}

fn text_config() -> HeartSearchConfig {
    HeartSearchConfig {
        placement_offsets_metres: vec![(28, 10)],
        scales_metres: vec![40],
        rotations_degrees: vec![-15],
        max_placements: 1,
        max_scales: 1,
        max_rotations: 1,
        max_anchor_matches: 16,
        max_provider_calls: 1,
        max_retained_points: 10_000,
        wall_time: Duration::from_secs(5),
        anchor_tolerance_metres: 8,
    }
}

fn pins(template: &VersionedTemplate) -> BenchmarkPins {
    BenchmarkPins {
        map_version: format!("cubbon-park-osm-v2+{}", SNAPSHOT_HASH.trim()),
        profile_version: "walking-v1".into(),
        routing_policy_version: "osm-pedestrian-policy-v2".into(),
        template_id: template.definition.id.clone(),
        template_version: template.definition.version,
        template_hash: template.definition_hash.clone(),
        algorithm_version: QUALITY_VERSION.into(),
    }
}

fn render_svg(candidate: &HeartCandidate, title: &str, orientation_degrees: i16) -> String {
    let origin = candidate.geometry[0];
    let longitude_scale = (f64::from(origin.latitude_e7) / 10_000_000.0)
        .to_radians()
        .cos();
    let radians = -f64::from(orientation_degrees).to_radians();
    let (sin, cos) = radians.sin_cos();
    let normalize = |point: GeoPointE7| {
        let east = f64::from(point.longitude_e7 - origin.longitude_e7) * longitude_scale;
        let north = f64::from(point.latitude_e7 - origin.latitude_e7);
        (east * cos - north * sin, east * sin + north * cos)
    };
    let normalized = candidate
        .geometry
        .iter()
        .copied()
        .map(normalize)
        .collect::<Vec<_>>();
    let min_east = normalized
        .iter()
        .map(|point| point.0)
        .reduce(f64::min)
        .unwrap();
    let max_east = normalized
        .iter()
        .map(|point| point.0)
        .reduce(f64::max)
        .unwrap();
    let min_north = normalized
        .iter()
        .map(|point| point.1)
        .reduce(f64::min)
        .unwrap();
    let max_north = normalized
        .iter()
        .map(|point| point.1)
        .reduce(f64::max)
        .unwrap();
    let east_span = (max_east - min_east).max(1.0);
    let north_span = (max_north - min_north).max(1.0);
    let drawing_scale = (640.0 / east_span).min(400.0 / north_span);
    let rendered_width = east_span * drawing_scale;
    let rendered_height = north_span * drawing_scale;
    let left = 40.0 + (640.0 - rendered_width) / 2.0;
    let top = 40.0 + (400.0 - rendered_height) / 2.0;
    let render_point = |point: GeoPointE7| {
        let (east, north) = normalize(point);
        let x = left + (east - min_east) * drawing_scale;
        let y = top + (max_north - north) * drawing_scale;
        format!("{x:.1},{y:.1}")
    };
    let mut full_trace = String::new();
    for point in &candidate.geometry {
        write!(full_trace, "{} ", render_point(*point)).unwrap();
    }
    let mut classified_segments = String::new();
    for occurrence in &candidate.occurrences {
        let colour = match occurrence.part {
            CandidatePart::Artwork => "#d43f3a",
            CandidatePart::Connector => "#d18b00",
            CandidatePart::Access | CandidatePart::Return => "#596b7a",
        };
        let mut segment = String::new();
        for point in
            &candidate.geometry[occurrence.begin_geometry_index..=occurrence.end_geometry_index]
        {
            write!(segment, "{} ", render_point(*point)).unwrap();
        }
        let dash = if occurrence.retraced {
            " stroke-dasharray=\"7 5\""
        } else {
            ""
        };
        write!(
            classified_segments,
            "<polyline points=\"{segment}\" fill=\"none\" stroke=\"{colour}\" stroke-width=\"4\" stroke-linejoin=\"round\" stroke-linecap=\"round\"{dash}/>"
        )
        .unwrap();
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"720\" height=\"500\" viewBox=\"0 0 720 500\"><rect width=\"720\" height=\"500\" fill=\"#f7f4ee\"/><text x=\"40\" y=\"26\" font-family=\"sans-serif\" font-size=\"16\">{title} - canonical full trace</text><polyline points=\"{full_trace}\" fill=\"none\" stroke=\"#b8b8b8\" stroke-width=\"2\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>{classified_segments}<circle cx=\"{start_x}\" cy=\"{start_y}\" r=\"7\" fill=\"#176b87\"/><text x=\"40\" y=\"465\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#d43f3a\">artwork</text><text x=\"105\" y=\"465\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#d18b00\">connector</text><text x=\"185\" y=\"465\" font-family=\"sans-serif\" font-size=\"12\" fill=\"#596b7a\">access / return</text><text x=\"40\" y=\"485\" font-family=\"sans-serif\" font-size=\"12\">Rigid orientation normalization only; full route retained; dashed segments retraced.</text></svg>\n",
        start_x = render_point(candidate.geometry[0])
            .split(',')
            .next()
            .unwrap(),
        start_y = render_point(candidate.geometry[0])
            .split(',')
            .nth(1)
            .unwrap(),
    )
}

#[tokio::main]
async fn main() {
    let graph = LocalOsmGraph::from_json(SNAPSHOT, SNAPSHOT_HASH.trim()).unwrap();
    let catalogue = embedded_catalogue().unwrap();
    let mut harness = BenchmarkHarness::new(BENCHMARK_SCHEMA_VERSION);

    for (case_id, template_id, start_node_id, orientation, target_metres, config, artifact) in [
        (
            "pilot-heart-001",
            "heart",
            13_459_508_937,
            60,
            1_300,
            heart_config(),
            "docs/evidence/ts-107-pilot-heart.svg",
        ),
        (
            "pilot-hi-001",
            "HI",
            665_065_740,
            -15,
            400,
            text_config(),
            "docs/evidence/ts-107-pilot-hi.svg",
        ),
    ] {
        let template = catalogue.get(template_id, 1).unwrap().clone();
        let request = request(&graph, start_node_id);
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
                Metres::new(target_metres).unwrap(),
                request.minimum_distance,
                request.maximum_distance,
            ),
        )
        .unwrap();
        fs::write(
            artifact,
            render_svg(&quality.candidate, &template.definition.name, orientation),
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
