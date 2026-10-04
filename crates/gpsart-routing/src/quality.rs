use crate::{CandidatePart, GeoPointE7, HeartCandidate};
use gpsart_domain::{
    Metres,
    templates::{Category, TraversalPart, VersionedTemplate},
};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::time::Duration;
use thiserror::Error;

pub const QUALITY_VERSION: &str = "candidate-quality-v1";
pub const WEIGHTS_VERSION: &str = "candidate-quality-weights-v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct QualityWeights {
    pub version: String,
    pub full_trace_error: u16,
    pub distance_error: u16,
    pub repeated_ratio: u16,
    pub connector_ratio: u16,
    pub manoeuvre_complexity: u16,
}

impl QualityWeights {
    pub fn v1() -> Self {
        Self {
            version: WEIGHTS_VERSION.into(),
            full_trace_error: 40,
            distance_error: 20,
            repeated_ratio: 15,
            connector_ratio: 15,
            manoeuvre_complexity: 10,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateQualityConfig {
    pub target_distance: Metres,
    pub minimum_distance: Metres,
    pub maximum_distance: Metres,
    pub weights: QualityWeights,
    pub strong_match_max_penalty: u64,
}

impl CandidateQualityConfig {
    pub fn v1(target_distance: Metres, minimum_distance: Metres, maximum_distance: Metres) -> Self {
        Self {
            target_distance,
            minimum_distance,
            maximum_distance,
            weights: QualityWeights::v1(),
            strong_match_max_penalty: 1_000_000,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IdentifyingFeatureCheck {
    pub feature: String,
    pub present: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateQualityMetrics {
    pub version: String,
    pub full_trace_error_metres: u32,
    pub distance_error_metres: u32,
    pub repeated_ratio_basis_points: u32,
    pub connector_ratio_basis_points: u32,
    pub manoeuvre_complexity: u32,
    pub identifying_feature_checks: Vec<IdentifyingFeatureCheck>,
    pub weights: QualityWeights,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityBand {
    StrongMatch,
    ApproximateMatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct UserQuality {
    pub quality_band: QualityBand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoredCandidate {
    pub candidate: HeartCandidate,
    pub metrics: CandidateQualityMetrics,
    pub quality_band: QualityBand,
    internal_penalty: u64,
}

impl ScoredCandidate {
    pub const fn user_quality(&self) -> UserQuality {
        UserQuality {
            quality_band: self.quality_band,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum QualityError {
    #[error("candidate distance is outside the immutable request bounds")]
    DistanceOutOfRange,
    #[error("candidate failed structural or legality validation")]
    InvalidCandidate,
    #[error("quality configuration is invalid")]
    InvalidConfiguration,
}

pub fn score_candidate(
    candidate: HeartCandidate,
    template: &VersionedTemplate,
    config: &CandidateQualityConfig,
) -> Result<ScoredCandidate, QualityError> {
    if config.minimum_distance.value() > config.target_distance.value()
        || config.target_distance.value() > config.maximum_distance.value()
    {
        return Err(QualityError::InvalidConfiguration);
    }
    if candidate.template_id != template.definition.id
        || candidate.template_version != template.definition.version
        || candidate.template_hash != template.definition_hash
    {
        return Err(QualityError::InvalidCandidate);
    }
    if !(config.minimum_distance.value()..=config.maximum_distance.value())
        .contains(&candidate.distance.value())
    {
        return Err(QualityError::DistanceOutOfRange);
    }
    validate_candidate(&candidate, template)?;

    let full_trace_error_metres = symmetric_trace_error(
        candidate.resemblance_trace(),
        &candidate.ordered_design_anchors,
    )?;
    let distance_error_metres = candidate
        .distance
        .value()
        .abs_diff(config.target_distance.value());
    let repeated_ratio_basis_points = ratio_basis_points(
        candidate.repeated_distance_metres,
        candidate.distance.value(),
    );
    let connector_ratio_basis_points = ratio_basis_points(
        candidate.connector_distance_metres,
        candidate.distance.value(),
    );
    let manoeuvre_complexity = bend_count(candidate.resemblance_trace());
    let feature_geometry_is_distinct = repeated_ratio_basis_points <= 5_000
        && connector_ratio_basis_points <= 5_000
        && full_trace_error_metres <= trace_span_metres(candidate.resemblance_trace()) / 3;
    let identifying_feature_checks = template
        .definition
        .identifying_features
        .iter()
        .map(|feature| IdentifyingFeatureCheck {
            feature: feature.clone(),
            // Each published feature depends on the validated ordered anchors remaining
            // distinguishable in the complete route. Heavy retracing, connecting travel,
            // or full-trace deviation fails the check instead of claiming recognisability.
            present: feature_geometry_is_distinct,
        })
        .collect();
    let metrics = CandidateQualityMetrics {
        version: QUALITY_VERSION.into(),
        full_trace_error_metres,
        distance_error_metres,
        repeated_ratio_basis_points,
        connector_ratio_basis_points,
        manoeuvre_complexity,
        identifying_feature_checks,
        weights: config.weights.clone(),
    };
    let internal_penalty = penalty(&metrics, config);
    let quality_band = if internal_penalty <= config.strong_match_max_penalty
        && metrics
            .identifying_feature_checks
            .iter()
            .all(|check| check.present)
    {
        QualityBand::StrongMatch
    } else {
        QualityBand::ApproximateMatch
    };
    Ok(ScoredCandidate {
        candidate,
        metrics,
        quality_band,
        internal_penalty,
    })
}

fn validate_candidate(
    candidate: &HeartCandidate,
    template: &VersionedTemplate,
) -> Result<(), QualityError> {
    let expected_anchor_count = template
        .complete_traversal()
        .into_iter()
        .flat_map(|part| match part {
            TraversalPart::Stroke(stroke) => stroke.points.iter(),
            TraversalPart::Transition(transition) => transition.points.iter(),
        })
        .map(|point| (point.x.to_bits(), point.y.to_bits()))
        .fold(Vec::new(), |mut points, point| {
            if points.last() != Some(&point) {
                points.push(point);
            }
            points
        })
        .len();
    let design_is_closed =
        candidate.ordered_design_anchors.first() == candidate.ordered_design_anchors.last();
    if candidate.geometry.len() < 2
        || candidate.ordered_design_anchors.len() != expected_anchor_count
        || design_is_closed != (template.definition.category == Category::Shape)
        || candidate.occurrences.is_empty()
        || candidate.geometry.first() != candidate.geometry.last()
        || candidate.closure_distance_metres > 5
    {
        return Err(QualityError::InvalidCandidate);
    }
    let mut expected_index = 0;
    let mut total = 0_u32;
    let mut connector = 0_u32;
    let mut repeated = 0_u32;
    let mut seen_edges = HashSet::new();
    for occurrence in &candidate.occurrences {
        if occurrence.begin_geometry_index != expected_index
            || occurrence.begin_geometry_index >= occurrence.end_geometry_index
            || occurrence.end_geometry_index >= candidate.geometry.len()
            || occurrence.retraced == seen_edges.insert(&occurrence.edge_id)
        {
            return Err(QualityError::InvalidCandidate);
        }
        expected_index = occurrence.end_geometry_index;
        total = total
            .checked_add(occurrence.distance.value())
            .ok_or(QualityError::InvalidCandidate)?;
        if occurrence.part == CandidatePart::Connector {
            connector = connector
                .checked_add(occurrence.distance.value())
                .ok_or(QualityError::InvalidCandidate)?;
        }
        if occurrence.retraced {
            repeated = repeated
                .checked_add(occurrence.distance.value())
                .ok_or(QualityError::InvalidCandidate)?;
        }
    }
    if expected_index != candidate.geometry.len() - 1
        || total != candidate.distance.value()
        || connector != candidate.connector_distance_metres
        || repeated != candidate.repeated_distance_metres
    {
        return Err(QualityError::InvalidCandidate);
    }
    Ok(())
}

fn symmetric_trace_error(trace: &[GeoPointE7], design: &[GeoPointE7]) -> Result<u32, QualityError> {
    fn directed(from: &[GeoPointE7], to: &[GeoPointE7]) -> Option<u32> {
        from.iter()
            .map(|point| point_to_polyline_metres(*point, to))
            .collect::<Option<Vec<_>>>()
            .map(|distances| distances.into_iter().max().unwrap_or(0))
    }
    directed(trace, design)
        .zip(directed(design, trace))
        .map(|(left, right)| left.max(right))
        .ok_or(QualityError::InvalidCandidate)
}

fn point_to_polyline_metres(point: GeoPointE7, polyline: &[GeoPointE7]) -> Option<u32> {
    const METRES_PER_DEGREE: f64 = 111_320.0;
    let latitude = f64::from(point.latitude_e7) / 10_000_000.0;
    let longitude_scale = METRES_PER_DEGREE * latitude.to_radians().cos();
    let local = |other: GeoPointE7| {
        (
            (f64::from(other.longitude_e7) - f64::from(point.longitude_e7)) / 10_000_000.0
                * longitude_scale,
            (f64::from(other.latitude_e7) - f64::from(point.latitude_e7)) / 10_000_000.0
                * METRES_PER_DEGREE,
        )
    };
    let distance = polyline
        .windows(2)
        .map(|segment| {
            let (ax, ay) = local(segment[0]);
            let (bx, by) = local(segment[1]);
            let dx = bx - ax;
            let dy = by - ay;
            let denominator = dx * dx + dy * dy;
            let t = if denominator <= f64::EPSILON {
                0.0
            } else {
                (-(ax * dx + ay * dy) / denominator).clamp(0.0, 1.0)
            };
            (ax + t * dx).hypot(ay + t * dy)
        })
        .reduce(f64::min)?;
    (distance.is_finite() && distance <= f64::from(u32::MAX)).then_some(distance.round() as u32)
}

fn trace_span_metres(trace: &[GeoPointE7]) -> u32 {
    let Some((&first, rest)) = trace.split_first() else {
        return 0;
    };
    let (min_lat, max_lat, min_lon, max_lon) = rest.iter().fold(
        (
            first.latitude_e7,
            first.latitude_e7,
            first.longitude_e7,
            first.longitude_e7,
        ),
        |(min_lat, max_lat, min_lon, max_lon), point| {
            (
                min_lat.min(point.latitude_e7),
                max_lat.max(point.latitude_e7),
                min_lon.min(point.longitude_e7),
                max_lon.max(point.longitude_e7),
            )
        },
    );
    point_to_polyline_metres(
        GeoPointE7::new(min_lat, min_lon),
        &[
            GeoPointE7::new(max_lat, max_lon),
            GeoPointE7::new(max_lat, max_lon),
        ],
    )
    .unwrap_or(0)
}

fn ratio_basis_points(part: u32, total: u32) -> u32 {
    u32::try_from((u64::from(part) * 10_000) / u64::from(total)).unwrap_or(10_000)
}

fn bend_count(points: &[GeoPointE7]) -> u32 {
    let simplified = simplify_collinear(points);
    u32::try_from(simplified.len().saturating_sub(2)).unwrap_or(u32::MAX)
}

fn penalty(metrics: &CandidateQualityMetrics, config: &CandidateQualityConfig) -> u64 {
    u64::from(metrics.full_trace_error_metres) * u64::from(config.weights.full_trace_error)
        + u64::from(metrics.distance_error_metres) * u64::from(config.weights.distance_error)
        + u64::from(metrics.repeated_ratio_basis_points) * u64::from(config.weights.repeated_ratio)
        + u64::from(metrics.connector_ratio_basis_points)
            * u64::from(config.weights.connector_ratio)
        + u64::from(metrics.manoeuvre_complexity) * u64::from(config.weights.manoeuvre_complexity)
}

pub fn rank_and_deduplicate(
    mut candidates: Vec<ScoredCandidate>,
    limit: usize,
) -> Vec<ScoredCandidate> {
    candidates.sort_by(compare_candidates);
    let mut selected: Vec<ScoredCandidate> = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .any(|existing| duplicates(existing, &candidate))
        {
            continue;
        }
        selected.push(candidate);
        if selected.len() == limit {
            break;
        }
    }
    selected
}

fn compare_candidates(left: &ScoredCandidate, right: &ScoredCandidate) -> Ordering {
    left.internal_penalty
        .cmp(&right.internal_penalty)
        .then_with(|| {
            left.metrics
                .full_trace_error_metres
                .cmp(&right.metrics.full_trace_error_metres)
        })
        .then_with(|| {
            left.metrics
                .distance_error_metres
                .cmp(&right.metrics.distance_error_metres)
        })
        .then_with(|| {
            left.candidate
                .geometry
                .len()
                .cmp(&right.candidate.geometry.len())
        })
        .then_with(|| traversal_key(left).cmp(&traversal_key(right)))
        .then_with(|| geometry_key(left).cmp(&geometry_key(right)))
}

fn duplicates(left: &ScoredCandidate, right: &ScoredCandidate) -> bool {
    traversal_key(left) == traversal_key(right)
        || simplify_collinear(&left.candidate.geometry)
            == simplify_collinear(&right.candidate.geometry)
}

fn traversal_key(candidate: &ScoredCandidate) -> Vec<&str> {
    candidate
        .candidate
        .occurrences
        .iter()
        .map(|item| item.edge_id.as_str())
        .collect()
}

fn geometry_key(candidate: &ScoredCandidate) -> Vec<(i32, i32)> {
    simplify_collinear(&candidate.candidate.geometry)
        .into_iter()
        .map(|point| (point.latitude_e7, point.longitude_e7))
        .collect()
}

fn simplify_collinear(points: &[GeoPointE7]) -> Vec<GeoPointE7> {
    let mut simplified = Vec::with_capacity(points.len());
    for &point in points {
        while simplified.len() >= 2 {
            let a: GeoPointE7 = simplified[simplified.len() - 2];
            let b: GeoPointE7 = simplified[simplified.len() - 1];
            let ab_lat = i128::from(b.latitude_e7) - i128::from(a.latitude_e7);
            let ab_lon = i128::from(b.longitude_e7) - i128::from(a.longitude_e7);
            let bc_lat = i128::from(point.latitude_e7) - i128::from(b.latitude_e7);
            let bc_lon = i128::from(point.longitude_e7) - i128::from(b.longitude_e7);
            if ab_lat * bc_lon != ab_lon * bc_lat {
                break;
            }
            simplified.pop();
        }
        if simplified.last() != Some(&point) {
            simplified.push(point);
        }
    }
    simplified
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenchmarkPins {
    pub map_version: String,
    pub profile_version: String,
    pub template_id: String,
    pub template_version: u32,
    pub template_hash: String,
    pub algorithm_version: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkOutcome {
    Match,
    NoMatch,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenchmarkRecord {
    pub case_id: String,
    pub pins: BenchmarkPins,
    pub latency_millis: u64,
    pub provider_calls: u32,
    pub outcome: BenchmarkOutcome,
    pub valid: bool,
    pub quality_band: Option<QualityBand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_metrics: Option<CandidateQualityMetrics>,
    pub route_artifact: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BenchmarkReport {
    pub schema_version: String,
    pub cases: Vec<BenchmarkRecord>,
}

pub struct BenchmarkHarness {
    schema_version: String,
    cases: Vec<BenchmarkRecord>,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BenchmarkError {
    #[error("benchmark record is missing an immutable version pin")]
    MissingVersionPin,
    #[error("benchmark result fields contradict its outcome")]
    InconsistentOutcome,
    #[error("benchmark case ids must be unique and non-empty")]
    DuplicateOrEmptyCase,
}

impl BenchmarkHarness {
    pub fn new(schema_version: impl Into<String>) -> Self {
        Self {
            schema_version: schema_version.into(),
            cases: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        case_id: impl Into<String>,
        pins: BenchmarkPins,
        latency: Duration,
        provider_calls: u32,
        outcome: BenchmarkOutcome,
        valid: bool,
        quality_band: Option<QualityBand>,
        route_artifact: Option<String>,
    ) -> Result<(), BenchmarkError> {
        let case_id = case_id.into();
        if case_id.is_empty() || self.cases.iter().any(|case| case.case_id == case_id) {
            return Err(BenchmarkError::DuplicateOrEmptyCase);
        }
        if pins.map_version.is_empty()
            || pins.profile_version.is_empty()
            || pins.template_id.is_empty()
            || pins.template_version == 0
            || pins.template_hash.is_empty()
            || pins.algorithm_version.is_empty()
        {
            return Err(BenchmarkError::MissingVersionPin);
        }
        if matches!(outcome, BenchmarkOutcome::Match) != quality_band.is_some()
            || (!matches!(outcome, BenchmarkOutcome::Match) && route_artifact.is_some())
            || (matches!(outcome, BenchmarkOutcome::Match) && !valid)
        {
            return Err(BenchmarkError::InconsistentOutcome);
        }
        self.cases.push(BenchmarkRecord {
            case_id,
            pins,
            latency_millis: u64::try_from(latency.as_millis()).unwrap_or(u64::MAX),
            provider_calls,
            outcome,
            valid,
            quality_band,
            quality_metrics: None,
            route_artifact,
        });
        Ok(())
    }

    pub fn record_scored_match(
        &mut self,
        case_id: impl Into<String>,
        pins: BenchmarkPins,
        latency: Duration,
        provider_calls: u32,
        scored: &ScoredCandidate,
        route_artifact: String,
    ) -> Result<(), BenchmarkError> {
        self.record(
            case_id,
            pins,
            latency,
            provider_calls,
            BenchmarkOutcome::Match,
            true,
            Some(scored.quality_band),
            Some(route_artifact),
        )?;
        self.cases
            .last_mut()
            .expect("record was just appended")
            .quality_metrics = Some(scored.metrics.clone());
        Ok(())
    }

    pub fn finish(mut self) -> Result<BenchmarkReport, BenchmarkError> {
        if self.schema_version.is_empty() {
            return Err(BenchmarkError::MissingVersionPin);
        }
        self.cases
            .sort_by(|left, right| left.case_id.cmp(&right.case_id));
        Ok(BenchmarkReport {
            schema_version: self.schema_version,
            cases: self.cases,
        })
    }
}
