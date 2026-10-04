use crate::{
    AttemptControl, ControlledGraph, GeoPointE7, LocalOsmGraph, ManoeuvreKind, RoutePath,
    RouteProvenance, RouteProvider, RouteQuery, RoutingError, TraversalDirection,
};
use gpsart_domain::templates::{
    Category, OrientationRule, TraversalPart, VersionedTemplate, embedded_catalogue,
};
use gpsart_domain::{Metres, RouteCandidateProfile, RouteRequestProfile};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::time::{Duration, Instant};
use thiserror::Error;

const EARTH_METRES_PER_DEGREE: f64 = 111_320.0;

pub trait AnchorMatch: Send + Sync {
    fn nearest_anchor(&self, target: GeoPointE7, tolerance_metres: u32) -> Option<GeoPointE7>;
}

impl AnchorMatch for ControlledGraph {
    fn nearest_anchor(&self, target: GeoPointE7, tolerance_metres: u32) -> Option<GeoPointE7> {
        nearest(target, self.nodes.values().copied(), tolerance_metres)
    }
}

impl AnchorMatch for LocalOsmGraph {
    fn nearest_anchor(&self, target: GeoPointE7, tolerance_metres: u32) -> Option<GeoPointE7> {
        nearest(
            target,
            self.node_ids_by_point.keys().copied(),
            tolerance_metres,
        )
    }
}

fn nearest(
    target: GeoPointE7,
    points: impl Iterator<Item = GeoPointE7>,
    tolerance_metres: u32,
) -> Option<GeoPointE7> {
    points
        .filter_map(|point| {
            if target == point {
                Some((point, 0))
            } else {
                crate::great_circle_metres(target, point).map(|distance| (point, distance))
            }
        })
        .filter(|(_, distance)| *distance <= tolerance_metres)
        .min_by_key(|(point, distance)| (*distance, point.latitude_e7, point.longitude_e7))
        .map(|(point, _)| point)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeartSearchRequest {
    pub profile: RouteRequestProfile,
    pub confirmed_start: GeoPointE7,
    pub minimum_distance: Metres,
    pub maximum_distance: Metres,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeartSearchConfig {
    pub placement_offsets_metres: Vec<(i32, i32)>,
    pub scales_metres: Vec<u32>,
    pub rotations_degrees: Vec<i16>,
    pub max_placements: usize,
    pub max_scales: usize,
    pub max_rotations: usize,
    pub max_anchor_matches: usize,
    pub max_provider_calls: usize,
    pub max_retained_points: usize,
    pub wall_time: Duration,
    pub anchor_tolerance_metres: u32,
}

pub type CuratedTextSearchRequest = HeartSearchRequest;
pub type CuratedTextSearchConfig = HeartSearchConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidatePart {
    Access,
    Artwork,
    Connector,
    Return,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraversalOccurrence {
    pub edge_id: String,
    pub direction: TraversalDirection,
    pub part: CandidatePart,
    pub retraced: bool,
    pub begin_geometry_index: usize,
    pub end_geometry_index: usize,
    pub distance: Metres,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeartCandidate {
    pub template_id: String,
    pub template_version: u32,
    pub template_hash: String,
    pub profile: RouteCandidateProfile,
    pub geometry: Vec<GeoPointE7>,
    pub ordered_design_anchors: Vec<GeoPointE7>,
    pub occurrences: Vec<TraversalOccurrence>,
    pub distance: Metres,
    pub connector_distance_metres: u32,
    pub repeated_distance_metres: u32,
    pub closure_distance_metres: u32,
    pub provenance: RouteProvenance,
}

impl HeartCandidate {
    pub fn resemblance_trace(&self) -> &[GeoPointE7] {
        &self.geometry
    }
}

pub type CuratedTextCandidate = HeartCandidate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatchOutcome {
    Match(Box<HeartCandidate>),
    NoMatch,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CandidateError {
    #[error("candidate search request or configuration is invalid")]
    InvalidRequest,
    #[error("candidate search was cancelled")]
    Cancelled,
    #[error("routing failed without establishing whether a match exists: {0}")]
    Routing(#[source] RoutingError),
}

pub struct CandidateSearch<'a, P, A> {
    provider: &'a P,
    anchors: &'a A,
}

#[derive(Clone, Copy)]
enum SearchKind {
    Heart,
    CuratedText,
}

impl<'a, P: RouteProvider, A: AnchorMatch> CandidateSearch<'a, P, A> {
    pub const fn new(provider: &'a P, anchors: &'a A) -> Self {
        Self { provider, anchors }
    }

    pub async fn heart(
        &self,
        template: &VersionedTemplate,
        request: &HeartSearchRequest,
        config: &HeartSearchConfig,
    ) -> Result<MatchOutcome, CandidateError> {
        validate_inputs(template, request, config, SearchKind::Heart)?;
        if config.max_provider_calls == 0 || config.wall_time.is_zero() {
            return Ok(MatchOutcome::NoMatch);
        }
        let control = AttemptControl::with_timeout(config.wall_time, config.max_provider_calls);
        self.search_with_control(template, request, config, &control, SearchKind::Heart)
            .await
    }

    pub async fn heart_with_control(
        &self,
        template: &VersionedTemplate,
        request: &HeartSearchRequest,
        config: &HeartSearchConfig,
        control: &AttemptControl,
    ) -> Result<MatchOutcome, CandidateError> {
        self.search_with_control(template, request, config, control, SearchKind::Heart)
            .await
    }

    pub async fn curated_text(
        &self,
        template: &VersionedTemplate,
        request: &CuratedTextSearchRequest,
        config: &CuratedTextSearchConfig,
    ) -> Result<MatchOutcome, CandidateError> {
        validate_inputs(template, request, config, SearchKind::CuratedText)?;
        if config.max_provider_calls == 0 || config.wall_time.is_zero() {
            return Ok(MatchOutcome::NoMatch);
        }
        let control = AttemptControl::with_timeout(config.wall_time, config.max_provider_calls);
        self.search_with_control(template, request, config, &control, SearchKind::CuratedText)
            .await
    }

    pub async fn curated_text_with_control(
        &self,
        template: &VersionedTemplate,
        request: &CuratedTextSearchRequest,
        config: &CuratedTextSearchConfig,
        control: &AttemptControl,
    ) -> Result<MatchOutcome, CandidateError> {
        self.search_with_control(template, request, config, control, SearchKind::CuratedText)
            .await
    }

    async fn search_with_control(
        &self,
        template: &VersionedTemplate,
        request: &HeartSearchRequest,
        config: &HeartSearchConfig,
        control: &AttemptControl,
        kind: SearchKind,
    ) -> Result<MatchOutcome, CandidateError> {
        validate_inputs(template, request, config, kind)?;
        if control.is_cancelled() {
            return Err(CandidateError::Cancelled);
        }
        if config.max_provider_calls == 0 || config.wall_time.is_zero() {
            return Ok(MatchOutcome::NoMatch);
        }
        let deadline = Instant::now()
            .checked_add(config.wall_time)
            .ok_or(CandidateError::InvalidRequest)?;
        let mut anchor_matches = 0_usize;
        let mut provider_calls = 0_usize;
        for &(east, north) in config
            .placement_offsets_metres
            .iter()
            .take(config.max_placements)
        {
            if control.is_cancelled() {
                return Err(CandidateError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Ok(MatchOutcome::NoMatch);
            }
            for &scale in config.scales_metres.iter().take(config.max_scales) {
                for &rotation in config.rotations_degrees.iter().take(config.max_rotations) {
                    if control.is_cancelled() {
                        return Err(CandidateError::Cancelled);
                    }
                    if Instant::now() >= deadline || provider_calls >= config.max_provider_calls {
                        return Ok(MatchOutcome::NoMatch);
                    }
                    let Some(projected) = project_and_match(
                        template,
                        request.confirmed_start,
                        east,
                        north,
                        scale,
                        rotation,
                        self.anchors,
                        config,
                        &mut anchor_matches,
                        deadline,
                        control,
                    )?
                    else {
                        continue;
                    };
                    let design_anchors = projected.anchors;
                    if matches!(kind, SearchKind::Heart)
                        && design_anchors.first() != design_anchors.last()
                    {
                        continue;
                    }
                    if matches!(kind, SearchKind::CuratedText)
                        && design_anchors.first() == design_anchors.last()
                    {
                        continue;
                    }
                    if control.is_cancelled() {
                        return Err(CandidateError::Cancelled);
                    }
                    if Instant::now() >= deadline {
                        return Ok(MatchOutcome::NoMatch);
                    }
                    let mut ordered = Vec::with_capacity(design_anchors.len() + 2);
                    ordered.push(request.confirmed_start);
                    ordered.extend(design_anchors.iter().copied());
                    ordered.push(request.confirmed_start);
                    if ordered.windows(2).any(|pair| pair[0] == pair[1]) {
                        continue;
                    }
                    let query = RouteQuery::from_points(request.profile, ordered);
                    provider_calls += 1;
                    let path = match self.provider.route(&query, control).await {
                        Ok(path) => path,
                        Err(RoutingError::NoPath) => continue,
                        Err(RoutingError::DeadlineExceeded | RoutingError::BudgetExhausted) => {
                            return Ok(MatchOutcome::NoMatch);
                        }
                        Err(RoutingError::Cancelled) => return Err(CandidateError::Cancelled),
                        Err(error) => return Err(CandidateError::Routing(error)),
                    };
                    if let Some(candidate) = validate_candidate(
                        template,
                        request,
                        config,
                        design_anchors,
                        projected.leg_parts,
                        path,
                    ) {
                        return Ok(MatchOutcome::Match(Box::new(candidate)));
                    }
                }
            }
        }
        Ok(MatchOutcome::NoMatch)
    }
}

fn validate_inputs(
    template: &VersionedTemplate,
    request: &HeartSearchRequest,
    config: &HeartSearchConfig,
    kind: SearchKind,
) -> Result<(), CandidateError> {
    let canonical =
        serde_json::to_vec(&template.definition).map_err(|_| CandidateError::InvalidRequest)?;
    let hash_is_valid = format!("{:x}", Sha256::digest(canonical)) == template.definition_hash
        && embedded_catalogue()
            .ok()
            .and_then(|catalogue| {
                catalogue
                    .get(&template.definition.id, template.definition.version)
                    .map(|published| published.definition_hash == template.definition_hash)
            })
            .unwrap_or(false);
    let template_is_valid = match kind {
        SearchKind::Heart => {
            template.definition.id == "heart"
                && template.definition.category == Category::Shape
                && template.definition.strokes.len() == 1
                && template.definition.strokes[0].points.len() >= 4
        }
        SearchKind::CuratedText => {
            template.definition.id == "HI"
                && template.definition.category == Category::Text
                && template.definition.strokes.len() > 1
                && template.definition.transitions.len() + 1 == template.definition.strokes.len()
                && template.definition.orientation.rule == OrientationRule::NorthUpBounded
                && !template.definition.orientation.mirror
                && !template.definition.orientation.stroke_reordering
        }
    };
    if !hash_is_valid
        || !template_is_valid
        || request.minimum_distance.value() > request.maximum_distance.value()
        || config.placement_offsets_metres.is_empty()
        || config.scales_metres.is_empty()
        || config.rotations_degrees.is_empty()
        || config.max_placements > config.placement_offsets_metres.len()
        || config.max_scales > config.scales_metres.len()
        || config.max_rotations > config.rotations_degrees.len()
        || config.scales_metres.contains(&0)
        || config.rotations_degrees.iter().any(|rotation| {
            f64::from(*rotation) < template.definition.orientation.min_degrees
                || f64::from(*rotation) > template.definition.orientation.max_degrees
        })
        || config.anchor_tolerance_metres == 0
    {
        return Err(CandidateError::InvalidRequest);
    }
    Ok(())
}

struct ProjectedDesign {
    anchors: Vec<GeoPointE7>,
    leg_parts: Vec<CandidatePart>,
}

#[allow(clippy::too_many_arguments)]
fn project_and_match<A: AnchorMatch>(
    template: &VersionedTemplate,
    origin: GeoPointE7,
    east: i32,
    north: i32,
    scale: u32,
    rotation: i16,
    anchors: &A,
    config: &HeartSearchConfig,
    anchor_matches: &mut usize,
    deadline: Instant,
    control: &AttemptControl,
) -> Result<Option<ProjectedDesign>, CandidateError> {
    let radians = f64::from(rotation).to_radians();
    let (sin, cos) = radians.sin_cos();
    let mut matched = Vec::new();
    let mut leg_parts = Vec::new();
    let mut previous_template_point = None;
    for part in template.complete_traversal() {
        let (points, candidate_part) = match part {
            TraversalPart::Stroke(stroke) => (&stroke.points, CandidatePart::Artwork),
            TraversalPart::Transition(transition) => (&transition.points, CandidatePart::Connector),
        };
        for point in points {
            let template_point = (point.x.to_bits(), point.y.to_bits());
            if previous_template_point == Some(template_point) {
                continue;
            }
            previous_template_point = Some(template_point);
            if control.is_cancelled() {
                return Err(CandidateError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            *anchor_matches = match anchor_matches.checked_add(1) {
                Some(value) => value,
                None => return Ok(None),
            };
            if *anchor_matches > config.max_anchor_matches {
                return Ok(None);
            }
            let local_east = (point.x - 0.5) * f64::from(scale);
            let local_north = (0.5 - point.y) * f64::from(scale);
            let rotated_east = local_east * cos - local_north * sin + f64::from(east);
            let rotated_north = local_east * sin + local_north * cos + f64::from(north);
            let Some(target) = offset_point(origin, rotated_east, rotated_north) else {
                return Ok(None);
            };
            let Some(anchor) = anchors.nearest_anchor(target, config.anchor_tolerance_metres)
            else {
                return Ok(None);
            };
            if matched.last() == Some(&anchor) {
                return Ok(None);
            }
            if !matched.is_empty() {
                leg_parts.push(candidate_part);
            }
            matched.push(anchor);
        }
    }
    if control.is_cancelled() {
        return Err(CandidateError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Ok(None);
    }
    Ok(
        (matched.len() >= 2 && leg_parts.len() + 1 == matched.len()).then_some(ProjectedDesign {
            anchors: matched,
            leg_parts,
        }),
    )
}

fn offset_point(origin: GeoPointE7, east: f64, north: f64) -> Option<GeoPointE7> {
    let latitude = f64::from(origin.latitude_e7) / 10_000_000.0;
    let latitude_delta = north / EARTH_METRES_PER_DEGREE;
    let longitude_scale = EARTH_METRES_PER_DEGREE * latitude.to_radians().cos();
    if longitude_scale.abs() < f64::EPSILON {
        return None;
    }
    let longitude_delta = east / longitude_scale;
    let latitude_e7 = f64::from(origin.latitude_e7) + latitude_delta * 10_000_000.0;
    let longitude_e7 = f64::from(origin.longitude_e7) + longitude_delta * 10_000_000.0;
    if !latitude_e7.is_finite()
        || !longitude_e7.is_finite()
        || latitude_e7 < f64::from(i32::MIN)
        || latitude_e7 > f64::from(i32::MAX)
        || longitude_e7 < f64::from(i32::MIN)
        || longitude_e7 > f64::from(i32::MAX)
    {
        return None;
    }
    Some(GeoPointE7::new(
        latitude_e7.round() as i32,
        longitude_e7.round() as i32,
    ))
}

fn validate_candidate(
    template: &VersionedTemplate,
    request: &HeartSearchRequest,
    config: &HeartSearchConfig,
    design_anchors: Vec<GeoPointE7>,
    design_leg_parts: Vec<CandidatePart>,
    path: RoutePath,
) -> Option<HeartCandidate> {
    if path.geometry.len() < 2
        || path.edges.is_empty()
        || path.geometry.len().checked_add(path.edges.len())? > config.max_retained_points
        || path.geometry.first() != Some(&request.confirmed_start)
    {
        return None;
    }
    let final_point = *path.geometry.last()?;
    let closure = if request.confirmed_start == final_point {
        0
    } else {
        crate::great_circle_metres(request.confirmed_start, final_point)?
    };
    if closure > 5
        || !(request.minimum_distance.value()..=request.maximum_distance.value())
            .contains(&path.distance.value())
    {
        return None;
    }
    let mut expected_geometry_index = 0_usize;
    let mut summed_distance = 0_u32;
    for edge in &path.edges {
        if edge.begin_geometry_index != expected_geometry_index
            || edge.begin_geometry_index >= edge.end_geometry_index
            || edge.end_geometry_index >= path.geometry.len()
            || edge
                .qualifications
                .contains(&crate::Qualification::ProviderAccessUnverified)
        {
            return None;
        }
        expected_geometry_index = edge.end_geometry_index;
        summed_distance = summed_distance.checked_add(edge.distance?.value())?;
    }
    if expected_geometry_index != path.geometry.len() - 1
        || summed_distance != path.distance.value()
    {
        return None;
    }
    let boundaries: Vec<_> = path
        .manoeuvres
        .iter()
        .filter_map(|manoeuvre| {
            matches!(manoeuvre.kind, ManoeuvreKind::Via).then_some(manoeuvre.geometry_index)
        })
        .collect();
    if boundaries.len() != design_anchors.len()
        || boundaries.windows(2).any(|pair| pair[0] >= pair[1])
        || boundaries
            .iter()
            .any(|index| *index == 0 || *index >= path.geometry.len() - 1)
        || boundaries
            .iter()
            .zip(&design_anchors)
            .any(|(index, anchor)| path.geometry[*index] != *anchor)
    {
        return None;
    }
    let mut seen = HashSet::new();
    if design_leg_parts.len() + 1 != design_anchors.len() {
        return None;
    }
    let mut occurrences = Vec::with_capacity(path.edges.len());
    let mut connector_distance_metres = 0_u32;
    let mut repeated_distance_metres = 0_u32;
    for edge in &path.edges {
        let leg = boundaries.partition_point(|boundary| *boundary <= edge.begin_geometry_index);
        let part = if leg == 0 {
            CandidatePart::Access
        } else if leg == design_anchors.len() {
            CandidatePart::Return
        } else {
            design_leg_parts[leg - 1]
        };
        let distance = edge.distance?;
        let retraced = !seen.insert(edge.edge_id.clone());
        if part == CandidatePart::Connector {
            connector_distance_metres = connector_distance_metres.checked_add(distance.value())?;
        }
        if retraced {
            repeated_distance_metres = repeated_distance_metres.checked_add(distance.value())?;
        }
        occurrences.push(TraversalOccurrence {
            edge_id: edge.edge_id.clone(),
            direction: edge.direction,
            part,
            retraced,
            begin_geometry_index: edge.begin_geometry_index,
            end_geometry_index: edge.end_geometry_index,
            distance,
        });
    }
    Some(HeartCandidate {
        template_id: template.definition.id.clone(),
        template_version: template.definition.version,
        template_hash: template.definition_hash.clone(),
        profile: RouteCandidateProfile::from_request(&request.profile),
        geometry: path.geometry,
        ordered_design_anchors: design_anchors,
        occurrences,
        distance: path.distance,
        connector_distance_metres,
        repeated_distance_metres,
        closure_distance_metres: closure,
        provenance: path.provenance,
    })
}
