use async_trait::async_trait;
use gpsart_domain::{ActivityProfile, Metres, RouteRequestProfile, templates::embedded_catalogue};
use gpsart_routing::{
    AnchorMatch, AttemptControl, CandidateError, CandidateQualityConfig, CandidateSearch,
    GeoPointE7, HeartSearchConfig, HeartSearchRequest, LocalOsmGraph, MatchOutcome, QualityBand,
    RoutePath, RouteProvider, RouteQuery, RoutingError, score_candidate,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicU32, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::Barrier;
use tokio::task::JoinSet;

const SNAPSHOT: &[u8] = include_bytes!("../data/bengaluru-central-osm-v3.json");
const SNAPSHOT_HASH: &str = include_str!("../data/bengaluru-central-osm-v3.sha256");
const WORKLOAD: &str = include_str!("../../../ops/performance/ts403-workload.json");

#[derive(Clone, Deserialize)]
struct WorkloadCase {
    id: String,
    category: String,
    activity: String,
    requested_distance_metres: u32,
    injection: String,
}

#[derive(Deserialize)]
struct Workload {
    concurrency: usize,
    cases: Vec<WorkloadCase>,
}

struct MeasuredProvider {
    graph: Arc<LocalOsmGraph>,
    calls: AtomicU32,
    elapsed_micros: AtomicU64,
    injection: String,
}

impl AnchorMatch for MeasuredProvider {
    fn nearest_anchor(&self, target: GeoPointE7, tolerance_metres: u32) -> Option<GeoPointE7> {
        self.graph.nearest_anchor(target, tolerance_metres)
    }
}

#[async_trait]
impl RouteProvider for MeasuredProvider {
    async fn route(
        &self,
        query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let started = Instant::now();
        let result = match self.injection.as_str() {
            "provider_timeout" => Err(RoutingError::DeadlineExceeded),
            "provider_quota" => Err(RoutingError::Unavailable),
            _ => self.graph.route(query, control).await,
        };
        self.elapsed_micros.fetch_add(
            started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        result
    }
}

#[derive(Debug)]
struct ResultRow {
    acknowledgement_us: u64,
    queue_us: u64,
    provider_us: u64,
    total_us: u64,
    provider_calls: u32,
    outcome: &'static str,
    quality: Option<&'static str>,
}

fn activity(value: &str) -> ActivityProfile {
    match value {
        "walking" => ActivityProfile::Walking,
        "running" => ActivityProfile::Running,
        "cycling" => ActivityProfile::Cycling,
        _ => panic!("unknown activity in reviewed workload"),
    }
}

fn config(category: &str) -> HeartSearchConfig {
    if category == "text" {
        HeartSearchConfig {
            placement_offsets_metres: vec![(66, 24)],
            scales_metres: vec![160],
            rotations_degrees: vec![-3],
            max_placements: 1,
            max_scales: 1,
            max_rotations: 1,
            max_anchor_matches: 16,
            max_provider_calls: 1,
            max_retained_points: 10_000,
            wall_time: Duration::from_secs(5),
            anchor_tolerance_metres: 12,
        }
    } else {
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
}

async fn execute(
    graph: Arc<LocalOsmGraph>,
    case: WorkloadCase,
    barrier: Arc<Barrier>,
    submitted: Instant,
) -> ResultRow {
    assert!(!case.id.is_empty());
    let acknowledged = Instant::now();
    let acknowledgement_us = acknowledged.duration_since(submitted).as_micros() as u64;
    barrier.wait().await;
    let started = Instant::now();
    let queue_us = started.duration_since(submitted).as_micros() as u64;
    let template_id = if case.category == "text" {
        "HI"
    } else {
        "heart"
    };
    let start_node = if case.category == "text" {
        10_238_269_050
    } else {
        13_459_508_937
    };
    let template = embedded_catalogue()
        .unwrap()
        .get(template_id, 1)
        .unwrap()
        .clone();
    let target = Metres::new(case.requested_distance_metres).unwrap();
    let tolerance = (case.requested_distance_metres / 5).max(1);
    let minimum = Metres::new(
        case.requested_distance_metres
            .saturating_sub(tolerance)
            .max(1),
    )
    .unwrap();
    let maximum = Metres::new(case.requested_distance_metres + tolerance).unwrap();
    let request = HeartSearchRequest {
        profile: RouteRequestProfile::new(activity(&case.activity)),
        confirmed_start: graph.point(start_node).unwrap(),
        minimum_distance: minimum,
        maximum_distance: maximum,
    };
    let provider = MeasuredProvider {
        graph,
        calls: AtomicU32::new(0),
        elapsed_micros: AtomicU64::new(0),
        injection: case.injection.clone(),
    };
    let search = CandidateSearch::new(&provider, &provider);
    let search_config = config(&case.category);
    let control =
        AttemptControl::with_timeout(Duration::from_secs(5), search_config.max_provider_calls);
    if case.injection == "cancel" {
        control.cancel();
    }
    let outcome = if case.category == "text" {
        search
            .curated_text_with_control(&template, &request, &search_config, &control)
            .await
    } else {
        search
            .heart_with_control(&template, &request, &search_config, &control)
            .await
    };
    let (outcome_name, quality) = match outcome {
        Ok(MatchOutcome::Match(candidate)) => {
            let scored = score_candidate(
                *candidate,
                &template,
                &CandidateQualityConfig::v1(target, minimum, maximum),
            )
            .unwrap();
            (
                "match",
                Some(match scored.quality_band {
                    QualityBand::StrongMatch => "strong_match",
                    QualityBand::ApproximateMatch => "approximate_match",
                }),
            )
        }
        Ok(MatchOutcome::NoMatch) if case.injection == "provider_timeout" => {
            ("provider_timeout", None)
        }
        Ok(MatchOutcome::NoMatch) => ("no_match", None),
        Err(CandidateError::Cancelled) => ("cancelled", None),
        Err(CandidateError::Routing(RoutingError::DeadlineExceeded)) => ("provider_timeout", None),
        Err(CandidateError::Routing(RoutingError::Unavailable)) => ("provider_quota", None),
        Err(_) => ("provider_error", None),
    };
    ResultRow {
        acknowledgement_us,
        queue_us,
        provider_us: provider.elapsed_micros.load(Ordering::Relaxed),
        total_us: started.elapsed().as_micros() as u64,
        provider_calls: provider.calls.load(Ordering::Relaxed),
        outcome: outcome_name,
        quality,
    }
}

fn percentile(rows: &[ResultRow], value: impl Fn(&ResultRow) -> u64, percentile: usize) -> u64 {
    let mut values = rows.iter().map(value).collect::<Vec<_>>();
    values.sort_unstable();
    values[(values.len() * percentile).div_ceil(100).saturating_sub(1)]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recorded pilot load; run explicitly with --ignored --nocapture"]
async fn twenty_concurrent_pinned_jobs_report_truthful_latency_and_outcomes() {
    let workload: Workload = serde_json::from_str(WORKLOAD).unwrap();
    assert_eq!(workload.concurrency, 20);
    assert_eq!(workload.cases.len(), 20);
    let graph = Arc::new(LocalOsmGraph::from_json(SNAPSHOT, SNAPSHOT_HASH.trim()).unwrap());
    let barrier = Arc::new(Barrier::new(workload.concurrency));
    let mut tasks = JoinSet::new();
    for case in workload.cases {
        let submitted = Instant::now();
        tasks.spawn(execute(graph.clone(), case, barrier.clone(), submitted));
    }
    let mut rows = Vec::new();
    while let Some(row) = tasks.join_next().await {
        rows.push(row.unwrap());
    }
    assert_eq!(rows.len(), 20);
    assert!(rows.iter().all(|row| row.total_us > 0));
    let count = |name| rows.iter().filter(|row| row.outcome == name).count();
    assert_eq!(count("cancelled"), 2);
    assert_eq!(count("provider_timeout"), 2);
    assert_eq!(count("provider_quota"), 2);
    let report = json!({
        "latency_microseconds": {
            "acknowledgement": { "p50": percentile(&rows, |row| row.acknowledgement_us, 50), "p95": percentile(&rows, |row| row.acknowledgement_us, 95), "p99": percentile(&rows, |row| row.acknowledgement_us, 99) },
            "queue": { "p50": percentile(&rows, |row| row.queue_us, 50), "p95": percentile(&rows, |row| row.queue_us, 95), "p99": percentile(&rows, |row| row.queue_us, 99) },
            "provider": { "p50": percentile(&rows, |row| row.provider_us, 50), "p95": percentile(&rows, |row| row.provider_us, 95), "p99": percentile(&rows, |row| row.provider_us, 99) },
            "total": { "p50": percentile(&rows, |row| row.total_us, 50), "p95": percentile(&rows, |row| row.total_us, 95), "p99": percentile(&rows, |row| row.total_us, 99) }
        },
        "outcomes": {
            "match": count("match"), "no_match": count("no_match"), "cancelled": count("cancelled"),
            "provider_timeout": count("provider_timeout"), "provider_quota": count("provider_quota"), "provider_error": count("provider_error")
        },
        "quality": {
            "strong_match": rows.iter().filter(|row| row.quality == Some("strong_match")).count(),
            "approximate_match": rows.iter().filter(|row| row.quality == Some("approximate_match")).count(),
            "not_applicable": rows.iter().filter(|row| row.quality.is_none()).count()
        },
        "provider_calls": {
            "zero": rows.iter().filter(|row| row.provider_calls == 0).count(),
            "one": rows.iter().filter(|row| row.provider_calls == 1).count(),
            "two_to_ten": rows.iter().filter(|row| (2..=10).contains(&row.provider_calls)).count(),
            "over_ten": rows.iter().filter(|row| row.provider_calls > 10).count()
        }
    });
    println!("TS403_REPORT={report}");
}
