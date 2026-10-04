use async_trait::async_trait;
use gpsart_domain::{
    AccessPolicy, ActivityProfileDefinition, Metres, RouteRequestProfile, profile_for,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::Notify;

#[cfg(test)]
mod candidate_quality_tests;
mod heart;
mod quality;

pub use heart::{
    AnchorMatch, CANDIDATE_LEGALITY_VERSION, CandidateError, CandidateLegalityEvidence,
    CandidatePart, CandidateSearch, CuratedTextCandidate, CuratedTextSearchConfig,
    CuratedTextSearchRequest, HeartCandidate, HeartSearchConfig, HeartSearchRequest, MatchOutcome,
    ROUTE_USE_ADVISORY, ROUTE_USE_ADVISORY_VERSION, TraversalOccurrence, ValidatedCandidate,
};
pub use quality::{
    BENCHMARK_SCHEMA_VERSION, BenchmarkError, BenchmarkHarness, BenchmarkOutcome, BenchmarkPins,
    BenchmarkRecord, BenchmarkReport, CandidateQualityConfig, CandidateQualityMetrics,
    FeatureCheckOutcome, IdentifyingFeatureCheck, QUALITY_VERSION, QualityBand, QualityError,
    QualityWeights, ScoredCandidate, UserQuality, WEIGHTS_VERSION, rank_and_deduplicate,
    score_candidate,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteQuery {
    pub profile: RouteRequestProfile,
    pub ordered_anchors: Vec<RouteAnchor>,
}

impl RouteQuery {
    pub fn new<'a>(
        profile: RouteRequestProfile,
        anchors: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        Self {
            profile,
            ordered_anchors: anchors
                .into_iter()
                .map(|id| RouteAnchor::NodeId(id.to_owned()))
                .collect(),
        }
    }

    pub fn from_points(
        profile: RouteRequestProfile,
        anchors: impl IntoIterator<Item = GeoPointE7>,
    ) -> Self {
        Self {
            profile,
            ordered_anchors: anchors.into_iter().map(RouteAnchor::Point).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Metadata<T> {
    Known(T),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Permitted,
    Forbidden,
    FootOnly,
    BicycleOnly,
    FootPermittedBicycleUnknown,
    FootForbiddenBicycleUnknown,
    FootUnknownBicyclePermitted,
    FootUnknownBicycleForbidden,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Both,
    ForwardOnly,
    ReverseOnly,
    Profiled {
        foot: DirectionRule,
        bicycle: DirectionRule,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectionRule {
    Both,
    ForwardOnly,
    ReverseOnly,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraversalDirection {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Surface {
    Paved,
    Unpaved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdgeMetadata {
    pub access: Metadata<Access>,
    pub direction: Metadata<Direction>,
    pub surface: Metadata<Surface>,
    pub elevation_metres: Metadata<i32>,
    pub steps: Metadata<bool>,
    pub ferry: Metadata<bool>,
    pub motorway: Metadata<bool>,
    pub motorroad: Metadata<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnMetadata {
    pub permitted: Metadata<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Qualification {
    AccessUnknown,
    SurfaceUnknown,
    ElevationUnknown,
    StepsUnknown,
    FerryUnknown,
    MotorwayUnknown,
    MotorroadUnknown,
    ProviderAccessUnverified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IneligibleReason {
    AccessForbidden,
    DirectionForbidden,
    DirectionUnknown,
    TurnForbidden,
    TurnUnknown,
    Steps,
    Ferry,
    Motorway,
    Motorroad,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Eligibility {
    Eligible { qualifications: Vec<Qualification> },
    Ineligible(IneligibleReason),
}

impl Eligibility {
    pub const fn is_eligible(&self) -> bool {
        matches!(self, Self::Eligible { .. })
    }
}

pub fn evaluate_edge(
    profile: ActivityProfileDefinition,
    edge: &EdgeMetadata,
    traversal: TraversalDirection,
) -> Eligibility {
    if edge.ferry == Metadata::Known(true) {
        return Eligibility::Ineligible(IneligibleReason::Ferry);
    }
    if edge.motorway == Metadata::Known(true) {
        return Eligibility::Ineligible(IneligibleReason::Motorway);
    }
    if edge.motorroad == Metadata::Known(true) {
        return Eligibility::Ineligible(IneligibleReason::Motorroad);
    }
    if profile.access_policy == AccessPolicy::Cycling && edge.steps == Metadata::Known(true) {
        return Eligibility::Ineligible(IneligibleReason::Steps);
    }

    let profile_access = access_for_profile(profile.access_policy, edge.access);
    if profile_access == ProfileAccess::Forbidden {
        return Eligibility::Ineligible(IneligibleReason::AccessForbidden);
    }

    match (
        direction_for_profile(profile.access_policy, edge.direction),
        traversal,
    ) {
        (DirectionRule::Unknown, _) => {
            return Eligibility::Ineligible(IneligibleReason::DirectionUnknown);
        }
        (DirectionRule::ForwardOnly, TraversalDirection::Reverse)
        | (DirectionRule::ReverseOnly, TraversalDirection::Forward) => {
            return Eligibility::Ineligible(IneligibleReason::DirectionForbidden);
        }
        _ => {}
    }

    let mut qualifications = Vec::new();
    if profile_access == ProfileAccess::Unknown {
        qualifications.push(Qualification::AccessUnknown);
    }
    if edge.surface == Metadata::Unknown {
        qualifications.push(Qualification::SurfaceUnknown);
    }
    if edge.elevation_metres == Metadata::Unknown {
        qualifications.push(Qualification::ElevationUnknown);
    }
    if edge.steps == Metadata::Unknown {
        qualifications.push(Qualification::StepsUnknown);
    }
    if edge.ferry == Metadata::Unknown {
        qualifications.push(Qualification::FerryUnknown);
    }
    if edge.motorway == Metadata::Unknown {
        qualifications.push(Qualification::MotorwayUnknown);
    }
    if edge.motorroad == Metadata::Unknown {
        qualifications.push(Qualification::MotorroadUnknown);
    }
    Eligibility::Eligible { qualifications }
}

const fn direction_for_profile(
    policy: AccessPolicy,
    direction: Metadata<Direction>,
) -> DirectionRule {
    match direction {
        Metadata::Unknown => DirectionRule::Unknown,
        Metadata::Known(Direction::Both) => DirectionRule::Both,
        Metadata::Known(Direction::ForwardOnly) => DirectionRule::ForwardOnly,
        Metadata::Known(Direction::ReverseOnly) => DirectionRule::ReverseOnly,
        Metadata::Known(Direction::Profiled { foot, bicycle }) => match policy {
            AccessPolicy::Foot => foot,
            AccessPolicy::Cycling => bicycle,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfileAccess {
    Permitted,
    Forbidden,
    Unknown,
}

fn access_for_profile(policy: AccessPolicy, access: Metadata<Access>) -> ProfileAccess {
    match (policy, access) {
        (_, Metadata::Unknown) => ProfileAccess::Unknown,
        (_, Metadata::Known(Access::Permitted))
        | (AccessPolicy::Foot, Metadata::Known(Access::FootOnly))
        | (AccessPolicy::Cycling, Metadata::Known(Access::BicycleOnly))
        | (AccessPolicy::Foot, Metadata::Known(Access::FootPermittedBicycleUnknown))
        | (AccessPolicy::Cycling, Metadata::Known(Access::FootUnknownBicyclePermitted)) => {
            ProfileAccess::Permitted
        }
        (_, Metadata::Known(Access::Forbidden))
        | (AccessPolicy::Cycling, Metadata::Known(Access::FootOnly))
        | (AccessPolicy::Foot, Metadata::Known(Access::BicycleOnly))
        | (AccessPolicy::Foot, Metadata::Known(Access::FootForbiddenBicycleUnknown))
        | (AccessPolicy::Cycling, Metadata::Known(Access::FootUnknownBicycleForbidden)) => {
            ProfileAccess::Forbidden
        }
        (
            AccessPolicy::Cycling,
            Metadata::Known(
                Access::FootPermittedBicycleUnknown | Access::FootForbiddenBicycleUnknown,
            ),
        )
        | (
            AccessPolicy::Foot,
            Metadata::Known(
                Access::FootUnknownBicyclePermitted | Access::FootUnknownBicycleForbidden,
            ),
        ) => ProfileAccess::Unknown,
    }
}

pub const fn evaluate_turn(
    _profile: ActivityProfileDefinition,
    turn: &TurnMetadata,
) -> Eligibility {
    match turn.permitted {
        Metadata::Known(true) => Eligibility::Eligible {
            qualifications: Vec::new(),
        },
        Metadata::Known(false) => Eligibility::Ineligible(IneligibleReason::TurnForbidden),
        Metadata::Unknown => Eligibility::Ineligible(IneligibleReason::TurnUnknown),
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GeoPointE7 {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

impl GeoPointE7 {
    pub const fn new(latitude_e7: i32, longitude_e7: i32) -> Self {
        Self {
            latitude_e7,
            longitude_e7,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteAnchor {
    NodeId(String),
    Point(GeoPointE7),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteEdge {
    pub edge_id: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub begin_geometry_index: usize,
    pub end_geometry_index: usize,
    pub direction: TraversalDirection,
    pub distance: Option<Metres>,
    pub qualifications: Vec<Qualification>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManoeuvreKind {
    Depart,
    Via,
    Arrive,
    Provider(u8),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manoeuvre {
    pub kind: ManoeuvreKind,
    pub geometry_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteProvenance {
    pub provider: String,
    pub data_version: Option<String>,
    pub attribution: Option<String>,
}

pub const CONTROLLED_GRAPH_POLICY_VERSION: &str = "controlled-graph-policy-v1";
pub const OSM_PEDESTRIAN_POLICY_VERSION: &str = "osm-pedestrian-policy-v2";
pub const VALHALLA_UNVERIFIED_POLICY_VERSION: &str = "valhalla-unverified-policy-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteLegalityEvidence {
    pub policy_version: String,
    pub access_verified: bool,
    pub direction_verified: bool,
    pub turns_verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePath {
    pub edges: Vec<RouteEdge>,
    pub geometry: Vec<GeoPointE7>,
    pub distance: Metres,
    pub manoeuvres: Vec<Manoeuvre>,
    pub provenance: RouteProvenance,
    pub legality: RouteLegalityEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlledNode {
    pub id: String,
    pub point: GeoPointE7,
}

impl ControlledNode {
    pub fn new(id: impl Into<String>, point: GeoPointE7) -> Self {
        Self {
            id: id.into(),
            point,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlledEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    distance_metres: u32,
    pub metadata: EdgeMetadata,
}

impl ControlledEdge {
    pub fn new(
        id: impl Into<String>,
        from: impl Into<String>,
        to: impl Into<String>,
        distance_metres: u32,
        metadata: EdgeMetadata,
    ) -> Self {
        Self {
            id: id.into(),
            from: from.into(),
            to: to.into(),
            distance_metres,
            metadata,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnRule {
    incoming_edge: String,
    outgoing_edge: String,
}

impl TurnRule {
    pub fn forbidden(incoming_edge: impl Into<String>, outgoing_edge: impl Into<String>) -> Self {
        Self {
            incoming_edge: incoming_edge.into(),
            outgoing_edge: outgoing_edge.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ControlledGraph {
    data_version: String,
    nodes: HashMap<String, GeoPointE7>,
    edges: Vec<ControlledEdge>,
    forbidden_turns: HashSet<(String, String)>,
}

impl ControlledGraph {
    pub fn new(
        data_version: impl Into<String>,
        nodes: Vec<ControlledNode>,
        edges: Vec<ControlledEdge>,
        turns: Vec<TurnRule>,
    ) -> Result<Self, RoutingError> {
        let nodes: HashMap<_, _> = nodes
            .into_iter()
            .map(|node| (node.id, node.point))
            .collect();
        if nodes.len() < 2
            || edges.iter().any(|edge| {
                edge.distance_metres == 0
                    || !nodes.contains_key(&edge.from)
                    || !nodes.contains_key(&edge.to)
            })
        {
            return Err(RoutingError::InvalidGraph);
        }
        let edge_ids: HashSet<_> = edges.iter().map(|edge| edge.id.as_str()).collect();
        if edge_ids.len() != edges.len()
            || turns.iter().any(|turn| {
                !edge_ids.contains(turn.incoming_edge.as_str())
                    || !edge_ids.contains(turn.outgoing_edge.as_str())
            })
        {
            return Err(RoutingError::InvalidGraph);
        }
        Ok(Self {
            data_version: data_version.into(),
            nodes,
            edges,
            forbidden_turns: turns
                .into_iter()
                .map(|turn| (turn.incoming_edge, turn.outgoing_edge))
                .collect(),
        })
    }

    fn calculate(&self, query: &RouteQuery) -> Result<RoutePath, RoutingError> {
        if query.ordered_anchors.len() < 2
            || query.ordered_anchors.iter().any(
                |anchor| !matches!(anchor, RouteAnchor::NodeId(id) if self.nodes.contains_key(id)),
            )
        {
            return Err(RoutingError::InvalidRequest);
        }
        let profile = profile_for(query.profile.activity());
        let mut traversals = Vec::new();
        let mut via_geometry_indexes = Vec::new();
        let mut incoming = None;
        for anchors in query.ordered_anchors.windows(2) {
            let (RouteAnchor::NodeId(start), RouteAnchor::NodeId(end)) = (&anchors[0], &anchors[1])
            else {
                return Err(RoutingError::InvalidRequest);
            };
            let segment = self.shortest_path(start, end, incoming.clone(), profile)?;
            incoming = segment.last().map(|edge| edge.edge_id.clone()).or(incoming);
            traversals.extend(segment);
            via_geometry_indexes.push(traversals.len());
        }
        if traversals.is_empty() {
            return Err(RoutingError::NoPath);
        }
        let mut geometry = Vec::with_capacity(traversals.len() + 1);
        geometry.push(
            self.nodes[traversals[0]
                .from
                .as_ref()
                .ok_or(RoutingError::InvalidGraph)?],
        );
        let mut total = 0_u32;
        for (index, traversal) in traversals.iter_mut().enumerate() {
            total = total
                .checked_add(
                    traversal
                        .distance
                        .ok_or(RoutingError::InvalidGraph)?
                        .value(),
                )
                .ok_or(RoutingError::DistanceOverflow)?;
            traversal.begin_geometry_index = index;
            traversal.end_geometry_index = index + 1;
            geometry.push(self.nodes[traversal.to.as_ref().ok_or(RoutingError::InvalidGraph)?]);
        }
        let mut manoeuvres = vec![Manoeuvre {
            kind: ManoeuvreKind::Depart,
            geometry_index: 0,
        }];
        for edge_count in via_geometry_indexes
            .iter()
            .take(via_geometry_indexes.len().saturating_sub(1))
        {
            manoeuvres.push(Manoeuvre {
                kind: ManoeuvreKind::Via,
                geometry_index: *edge_count,
            });
        }
        manoeuvres.push(Manoeuvre {
            kind: ManoeuvreKind::Arrive,
            geometry_index: geometry.len() - 1,
        });
        Ok(RoutePath {
            edges: traversals,
            geometry,
            distance: Metres::new(total).map_err(|_| RoutingError::DistanceOverflow)?,
            manoeuvres,
            provenance: RouteProvenance {
                provider: "controlled-graph".into(),
                data_version: Some(self.data_version.clone()),
                attribution: None,
            },
            legality: RouteLegalityEvidence {
                policy_version: CONTROLLED_GRAPH_POLICY_VERSION.into(),
                access_verified: true,
                direction_verified: true,
                turns_verified: true,
            },
        })
    }

    fn rewrite_point_query(&self, query: &RouteQuery) -> Result<RouteQuery, RoutingError> {
        let ordered_anchors = query
            .ordered_anchors
            .iter()
            .map(|anchor| match anchor {
                RouteAnchor::NodeId(id) if self.nodes.contains_key(id) => {
                    Ok(RouteAnchor::NodeId(id.clone()))
                }
                RouteAnchor::Point(point) => self
                    .nodes
                    .iter()
                    .find_map(|(id, candidate)| (candidate == point).then(|| id.clone()))
                    .map(RouteAnchor::NodeId)
                    .ok_or(RoutingError::InvalidRequest),
                RouteAnchor::NodeId(_) => Err(RoutingError::InvalidRequest),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RouteQuery {
            profile: query.profile,
            ordered_anchors,
        })
    }

    fn shortest_path(
        &self,
        start: &str,
        end: &str,
        initial_incoming: Option<String>,
        profile: ActivityProfileDefinition,
    ) -> Result<Vec<RouteEdge>, RoutingError> {
        type State = (String, Option<String>);
        let initial = (start.to_owned(), initial_incoming);
        let mut costs = HashMap::from([(initial.clone(), 0_u32)]);
        let mut visited = HashSet::new();
        let mut previous: HashMap<State, (State, RouteEdge)> = HashMap::new();
        let terminal = loop {
            let Some((state, cost)) = costs
                .iter()
                .filter(|(state, _)| !visited.contains(*state))
                .min_by_key(|(_, cost)| **cost)
                .map(|(state, cost)| (state.clone(), *cost))
            else {
                return Err(RoutingError::NoPath);
            };
            if state.0 == end {
                break state;
            }
            visited.insert(state.clone());
            for edge in &self.edges {
                let (next, direction) = if edge.from == state.0 {
                    (&edge.to, TraversalDirection::Forward)
                } else if edge.to == state.0 {
                    (&edge.from, TraversalDirection::Reverse)
                } else {
                    continue;
                };
                let Eligibility::Eligible { qualifications } =
                    evaluate_edge(profile, &edge.metadata, direction)
                else {
                    continue;
                };
                if state.1.as_ref().is_some_and(|incoming| {
                    self.forbidden_turns
                        .contains(&(incoming.clone(), edge.id.clone()))
                }) {
                    continue;
                }
                let Some(next_cost) = cost.checked_add(edge.distance_metres) else {
                    continue;
                };
                let next_state = (next.clone(), Some(edge.id.clone()));
                if costs
                    .get(&next_state)
                    .is_none_or(|known| next_cost < *known)
                {
                    costs.insert(next_state.clone(), next_cost);
                    previous.insert(
                        next_state,
                        (
                            state.clone(),
                            RouteEdge {
                                edge_id: edge.id.clone(),
                                from: Some(state.0.clone()),
                                to: Some(next.clone()),
                                begin_geometry_index: 0,
                                end_geometry_index: 0,
                                direction,
                                distance: Some(
                                    Metres::new(edge.distance_metres)
                                        .map_err(|_| RoutingError::InvalidGraph)?,
                                ),
                                qualifications,
                            },
                        ),
                    );
                }
            }
        };
        let mut cursor = terminal;
        let mut path = Vec::new();
        while cursor != initial {
            let (prior, traversal) = previous.remove(&cursor).ok_or(RoutingError::NoPath)?;
            path.push(traversal);
            cursor = prior;
        }
        path.reverse();
        Ok(path)
    }
}

#[derive(Clone, Debug)]
pub struct LocalOsmGraph {
    graph: ControlledGraph,
    points_by_osm_id: HashMap<u64, GeoPointE7>,
    node_ids_by_point: HashMap<GeoPointE7, String>,
    data_version: String,
    attribution: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OsmSnapshot {
    schema_version: u32,
    snapshot_id: String,
    source: OsmSnapshotSource,
    nodes: Vec<OsmSnapshotNode>,
    ways: Vec<OsmSnapshotWay>,
    forbidden_turns: Vec<OsmSnapshotTurn>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OsmSnapshotSource {
    api: String,
    url: String,
    retrieved_at: String,
    raw_sha256: String,
    attribution: String,
    license: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OsmSnapshotNode {
    id: u64,
    version: u32,
    timestamp: String,
    latitude_e7: i32,
    longitude_e7: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OsmSnapshotWay {
    id: u64,
    version: u32,
    timestamp: String,
    nodes: Vec<u64>,
    tags: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OsmSnapshotTurn {
    relation_id: u64,
    relation_version: u32,
    incoming_edge: String,
    outgoing_edge: String,
}

impl LocalOsmGraph {
    pub fn from_json(bytes: &[u8], expected_sha256: &str) -> Result<Self, RoutingError> {
        let actual_sha256 = Self::sha256_hex(bytes);
        if !is_sha256(expected_sha256) || !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
            return Err(RoutingError::SnapshotChecksumMismatch);
        }
        let snapshot: OsmSnapshot =
            serde_json::from_slice(bytes).map_err(|_| RoutingError::InvalidSnapshot)?;
        validate_snapshot_header(&snapshot)?;

        let mut nodes = Vec::with_capacity(snapshot.nodes.len());
        let mut points_by_osm_id = HashMap::with_capacity(snapshot.nodes.len());
        let mut node_ids_by_point = HashMap::with_capacity(snapshot.nodes.len());
        for node in snapshot.nodes {
            if node.id == 0
                || node.version == 0
                || !is_utc_timestamp(&node.timestamp)
                || !(-900_000_000..=900_000_000).contains(&node.latitude_e7)
                || !(-1_800_000_000..=1_800_000_000).contains(&node.longitude_e7)
            {
                return Err(RoutingError::InvalidSnapshot);
            }
            let point = GeoPointE7::new(node.latitude_e7, node.longitude_e7);
            let id = osm_node_id(node.id);
            if points_by_osm_id.insert(node.id, point).is_some()
                || node_ids_by_point.insert(point, id.clone()).is_some()
            {
                return Err(RoutingError::InvalidSnapshot);
            }
            nodes.push(ControlledNode::new(id, point));
        }
        if nodes.len() < 2 {
            return Err(RoutingError::InvalidSnapshot);
        }

        let mut seen_ways = HashSet::new();
        let mut edges = Vec::new();
        for way in snapshot.ways {
            if way.id == 0
                || way.version == 0
                || !is_utc_timestamp(&way.timestamp)
                || way.nodes.len() < 2
                || !seen_ways.insert(way.id)
            {
                return Err(RoutingError::InvalidSnapshot);
            }
            let metadata = osm_edge_metadata(&way.tags);
            for (segment_index, pair) in way.nodes.windows(2).enumerate() {
                let from = points_by_osm_id
                    .get(&pair[0])
                    .ok_or(RoutingError::InvalidSnapshot)?;
                let to = points_by_osm_id
                    .get(&pair[1])
                    .ok_or(RoutingError::InvalidSnapshot)?;
                let distance =
                    great_circle_metres(*from, *to).ok_or(RoutingError::InvalidSnapshot)?;
                edges.push(ControlledEdge::new(
                    osm_edge_id(way.id, way.version, segment_index),
                    osm_node_id(pair[0]),
                    osm_node_id(pair[1]),
                    distance,
                    metadata.clone(),
                ));
            }
        }
        if edges.is_empty() {
            return Err(RoutingError::InvalidSnapshot);
        }

        let edge_ids: HashSet<_> = edges.iter().map(|edge| edge.id.as_str()).collect();
        let mut relation_ids = HashSet::new();
        let turns = snapshot
            .forbidden_turns
            .into_iter()
            .map(|turn| {
                if turn.relation_id == 0
                    || turn.relation_version == 0
                    || !relation_ids.insert(turn.relation_id)
                    || !edge_ids.contains(turn.incoming_edge.as_str())
                    || !edge_ids.contains(turn.outgoing_edge.as_str())
                {
                    return Err(RoutingError::InvalidSnapshot);
                }
                Ok(TurnRule::forbidden(turn.incoming_edge, turn.outgoing_edge))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let data_version = format!("{}:sha256:{}", snapshot.snapshot_id, actual_sha256);
        let attribution = format!(
            "{} ({})",
            snapshot.source.attribution,
            snapshot.source.license.replace('-', " ")
        );
        let graph = ControlledGraph::new(data_version.clone(), nodes, edges, turns)
            .map_err(|_| RoutingError::InvalidSnapshot)?;
        Ok(Self {
            graph,
            points_by_osm_id,
            node_ids_by_point,
            data_version,
            attribution,
        })
    }

    pub fn sha256_hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    pub fn point(&self, osm_node_id: u64) -> Option<GeoPointE7> {
        self.points_by_osm_id.get(&osm_node_id).copied()
    }

    fn rewrite_query(&self, query: &RouteQuery) -> Result<RouteQuery, RoutingError> {
        let ordered_anchors = query
            .ordered_anchors
            .iter()
            .map(|anchor| match anchor {
                RouteAnchor::NodeId(id) => Ok(RouteAnchor::NodeId(id.clone())),
                RouteAnchor::Point(point) => self
                    .node_ids_by_point
                    .get(point)
                    .cloned()
                    .map(RouteAnchor::NodeId)
                    .ok_or(RoutingError::InvalidRequest),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RouteQuery {
            profile: query.profile,
            ordered_anchors,
        })
    }
}

fn validate_snapshot_header(snapshot: &OsmSnapshot) -> Result<(), RoutingError> {
    if snapshot.schema_version != 1
        || snapshot.snapshot_id.trim().is_empty()
        || snapshot.source.api != "OpenStreetMap API 0.6"
        || !snapshot
            .source
            .url
            .starts_with("https://api.openstreetmap.org/api/0.6/")
        || !is_utc_timestamp(&snapshot.source.retrieved_at)
        || !is_sha256(&snapshot.source.raw_sha256)
        || snapshot.source.attribution != "OpenStreetMap contributors"
        || snapshot.source.license != "ODbL-1.0"
    {
        return Err(RoutingError::InvalidSnapshot);
    }
    Ok(())
}

fn is_utc_timestamp(value: &str) -> bool {
    value.len() >= 20 && value.ends_with('Z') && value.contains('T')
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn osm_node_id(id: u64) -> String {
    format!("osm-node/{id}")
}

fn osm_edge_id(way_id: u64, way_version: u32, segment_index: usize) -> String {
    format!("osm-way/{way_id}/v{way_version}/segment/{segment_index}")
}

fn osm_edge_metadata(tags: &HashMap<String, String>) -> EdgeMetadata {
    EdgeMetadata {
        access: osm_access(tags),
        direction: osm_direction(tags),
        surface: osm_surface(tags),
        elevation_metres: Metadata::Unknown,
        steps: match tags.get("highway").map(String::as_str) {
            Some("steps") => Metadata::Known(true),
            Some(_) => Metadata::Known(false),
            None => Metadata::Unknown,
        },
        ferry: match tags.get("route").map(String::as_str) {
            Some("ferry") => Metadata::Known(true),
            Some(_) => Metadata::Known(false),
            None => Metadata::Unknown,
        },
        motorway: match tags.get("highway").map(String::as_str) {
            Some("motorway" | "motorway_link") => Metadata::Known(true),
            Some(_) => Metadata::Known(false),
            None => Metadata::Unknown,
        },
        motorroad: osm_yes_no(tags.get("motorroad").map(String::as_str)),
    }
}

fn osm_access(tags: &HashMap<String, String>) -> Metadata<Access> {
    let has_general_conditional = tags.contains_key("access:conditional");
    let general = osm_permission(tags.get("access").map(String::as_str));
    let foot_default = match tags.get("highway").map(String::as_str) {
        Some("footway" | "pedestrian" | "steps") => Some(true),
        _ => general,
    };
    let foot = osm_profile_permission(
        tags,
        "foot",
        "foot:conditional",
        foot_default,
        has_general_conditional,
    );
    let bicycle = osm_profile_permission(
        tags,
        "bicycle",
        "bicycle:conditional",
        general,
        has_general_conditional,
    );
    match (foot, bicycle) {
        (Some(true), Some(true)) => Metadata::Known(Access::Permitted),
        (Some(true), Some(false)) => Metadata::Known(Access::FootOnly),
        (Some(false), Some(true)) => Metadata::Known(Access::BicycleOnly),
        (Some(false), Some(false)) => Metadata::Known(Access::Forbidden),
        (Some(true), None) => Metadata::Known(Access::FootPermittedBicycleUnknown),
        (Some(false), None) => Metadata::Known(Access::FootForbiddenBicycleUnknown),
        (None, Some(true)) => Metadata::Known(Access::FootUnknownBicyclePermitted),
        (None, Some(false)) => Metadata::Known(Access::FootUnknownBicycleForbidden),
        (None, None) => Metadata::Unknown,
    }
}

fn osm_profile_permission(
    tags: &HashMap<String, String>,
    profile_key: &str,
    conditional_key: &str,
    general: Option<bool>,
    has_general_conditional: bool,
) -> Option<bool> {
    if tags.contains_key(conditional_key) {
        None
    } else if let Some(value) = tags.get(profile_key) {
        osm_permission(Some(value))
    } else if has_general_conditional {
        None
    } else {
        general
    }
}

fn osm_permission(value: Option<&str>) -> Option<bool> {
    match value {
        Some("yes" | "permissive" | "designated") => Some(true),
        Some("no" | "private") => Some(false),
        _ => None,
    }
}

fn osm_direction(tags: &HashMap<String, String>) -> Metadata<Direction> {
    let path_like = matches!(
        tags.get("highway").map(String::as_str),
        Some("footway" | "path" | "pedestrian" | "steps")
    );
    let general = if tags.contains_key("oneway:conditional") {
        DirectionRule::Unknown
    } else {
        osm_direction_rule(tags.get("oneway").map(String::as_str)).unwrap_or(DirectionRule::Both)
    };
    let foot = if tags.contains_key("oneway:foot:conditional")
        || (path_like && tags.contains_key("oneway:conditional"))
    {
        DirectionRule::Unknown
    } else if let Some(rule) = osm_direction_rule(tags.get("oneway:foot").map(String::as_str)) {
        rule
    } else if path_like && tags.contains_key("oneway") {
        // General `oneway` on path-like ways is used inconsistently. Require
        // `oneway:foot` before claiming a pedestrian direction.
        DirectionRule::Unknown
    } else {
        // A road's general `oneway` is a vehicle restriction. Pedestrian
        // direction is bidirectional unless a foot-specific tag says otherwise.
        DirectionRule::Both
    };
    let bicycle = if tags.contains_key("oneway:bicycle:conditional") {
        DirectionRule::Unknown
    } else {
        osm_direction_rule(tags.get("oneway:bicycle").map(String::as_str)).unwrap_or(general)
    };
    Metadata::Known(Direction::Profiled { foot, bicycle })
}

fn osm_direction_rule(value: Option<&str>) -> Option<DirectionRule> {
    match value {
        Some("yes" | "1" | "true") => Some(DirectionRule::ForwardOnly),
        Some("-1" | "reverse") => Some(DirectionRule::ReverseOnly),
        Some("no" | "0" | "false") => Some(DirectionRule::Both),
        _ => None,
    }
}

fn osm_surface(tags: &HashMap<String, String>) -> Metadata<Surface> {
    match tags.get("surface").map(String::as_str) {
        Some("asphalt" | "concrete" | "concrete:plates" | "paving_stones" | "sett" | "paved") => {
            Metadata::Known(Surface::Paved)
        }
        Some(
            "compacted" | "dirt" | "earth" | "fine_gravel" | "grass" | "gravel" | "ground" | "mud"
            | "sand" | "unpaved" | "woodchips",
        ) => Metadata::Known(Surface::Unpaved),
        _ => Metadata::Unknown,
    }
}

fn osm_yes_no(value: Option<&str>) -> Metadata<bool> {
    match value {
        Some("yes" | "1" | "true") => Metadata::Known(true),
        Some("no" | "0" | "false") => Metadata::Known(false),
        _ => Metadata::Unknown,
    }
}

fn great_circle_metres(from: GeoPointE7, to: GeoPointE7) -> Option<u32> {
    const EARTH_MEAN_RADIUS_METRES: f64 = 6_371_008.8;
    let latitude_1 = (from.latitude_e7 as f64 / 10_000_000.0).to_radians();
    let latitude_2 = (to.latitude_e7 as f64 / 10_000_000.0).to_radians();
    let delta_latitude = latitude_2 - latitude_1;
    let mut delta_longitude_degrees =
        (f64::from(to.longitude_e7) - f64::from(from.longitude_e7)) / 10_000_000.0;
    if delta_longitude_degrees > 180.0 {
        delta_longitude_degrees -= 360.0;
    } else if delta_longitude_degrees < -180.0 {
        delta_longitude_degrees += 360.0;
    }
    let delta_longitude = delta_longitude_degrees.to_radians();
    let haversine = (delta_latitude / 2.0).sin().powi(2)
        + latitude_1.cos() * latitude_2.cos() * (delta_longitude / 2.0).sin().powi(2);
    let metres = 2.0 * EARTH_MEAN_RADIUS_METRES * haversine.clamp(0.0, 1.0).sqrt().asin();
    let rounded = metres.round();
    if !rounded.is_finite() || rounded < 1.0 || rounded > u32::MAX as f64 {
        return None;
    }
    Some(rounded as u32)
}

#[derive(Debug)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

#[derive(Clone, Debug)]
pub struct AttemptControl {
    deadline: Instant,
    attempts_remaining: Arc<AtomicUsize>,
    cancellation: Arc<CancellationState>,
}

impl AttemptControl {
    pub fn with_timeout(timeout: Duration, attempt_budget: usize) -> Self {
        Self {
            deadline: Instant::now() + timeout,
            attempts_remaining: Arc::new(AtomicUsize::new(attempt_budget)),
            cancellation: Arc::new(CancellationState {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            }),
        }
    }

    pub fn cancel(&self) {
        self.cancellation.cancelled.store(true, Ordering::Release);
        self.cancellation.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.cancelled.load(Ordering::Acquire)
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.cancellation.notify.notified();
            if self.cancellation.cancelled.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    pub async fn run<F>(&self, future: F) -> Result<RoutePath, RoutingError>
    where
        F: Future<Output = Result<RoutePath, RoutingError>>,
    {
        if self.cancellation.cancelled.load(Ordering::Acquire) {
            return Err(RoutingError::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(RoutingError::DeadlineExceeded);
        }
        self.attempts_remaining
            .try_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
                remaining.checked_sub(1)
            })
            .map_err(|_| RoutingError::BudgetExhausted)?;
        let deadline = tokio::time::Instant::from_std(self.deadline);
        tokio::select! {
            biased;
            () = self.cancelled() => Err(RoutingError::Cancelled),
            () = tokio::time::sleep_until(deadline) => Err(RoutingError::DeadlineExceeded),
            result = future => result,
        }
    }
}

#[async_trait]
pub trait ValhallaTransport: Send + Sync {
    async fn post_route(&self, request: Value) -> Result<Value, RoutingError>;
    async fn post_trace_attributes(&self, request: Value) -> Result<Value, RoutingError>;
}

pub struct ValhallaAdapter<T> {
    transport: T,
    data_version: Option<String>,
    attribution: String,
}

impl<T> ValhallaAdapter<T> {
    pub fn new(transport: T, data_version: Option<String>, attribution: impl Into<String>) -> Self {
        Self {
            transport,
            data_version,
            attribution: attribution.into(),
        }
    }
}

#[async_trait]
impl<T: ValhallaTransport> RouteProvider for ValhallaAdapter<T> {
    async fn route(
        &self,
        query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        control
            .run(async {
                let points: Vec<_> = query
                    .ordered_anchors
                    .iter()
                    .map(|anchor| match anchor {
                        RouteAnchor::Point(point) => Ok(*point),
                        RouteAnchor::NodeId(_) => Err(RoutingError::InvalidRequest),
                    })
                    .collect::<Result<_, _>>()?;
                if points.len() < 2 {
                    return Err(RoutingError::InvalidRequest);
                }
                let last = points.len() - 1;
                let locations: Vec<_> = points
                    .iter()
                    .enumerate()
                    .map(|(index, point)| {
                        json!({
                            "lat": point.latitude_e7 as f64 / 10_000_000.0,
                            "lon": point.longitude_e7 as f64 / 10_000_000.0,
                            "type": if index == 0 || index == last { "break" } else { "through" },
                        })
                    })
                    .collect();
                let costing = match query.profile.activity() {
                    gpsart_domain::ActivityProfile::Cycling => "bicycle",
                    gpsart_domain::ActivityProfile::Walking
                    | gpsart_domain::ActivityProfile::Running => "pedestrian",
                };
                let response = self
                    .transport
                    .post_route(json!({
                        "locations": locations,
                        "costing": costing,
                        "units": "kilometers"
                    }))
                    .await?;
                let trip = response
                    .get("trip")
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                let legs = trip
                    .get("legs")
                    .and_then(Value::as_array)
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                if legs.len() != 1 {
                    return Err(RoutingError::Unsupported);
                }
                let leg = &legs[0];
                let encoded_shape = leg
                    .get("shape")
                    .and_then(Value::as_str)
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                let geometry = decode_polyline6(encoded_shape)?;
                let trace = self
                    .transport
                    .post_trace_attributes(json!({
                        "encoded_polyline": encoded_shape,
                        "costing": costing,
                        "shape_match": "edge_walk",
                        "filters": {
                            "action": "include",
                            "attributes": [
                                "edge.id", "edge.length", "edge.begin_shape_index",
                                "edge.end_shape_index", "edge.traversability", "edge.use",
                                "edge.way_id"
                            ]
                        }
                    }))
                    .await?;
                let trace_edges = trace
                    .get("edges")
                    .and_then(Value::as_array)
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                let mut edges = Vec::with_capacity(trace_edges.len());
                let mut expected_begin = 0_usize;
                let mut traced_kilometres = 0.0_f64;
                for edge in trace_edges {
                    let id = edge
                        .get("id")
                        .and_then(Value::as_u64)
                        .ok_or(RoutingError::InvalidProviderResponse)?;
                    let begin = json_usize(edge, "begin_shape_index")?;
                    let end = json_usize(edge, "end_shape_index")?;
                    if begin != expected_begin || begin >= end || end >= geometry.len() {
                        return Err(RoutingError::InvalidProviderResponse);
                    }
                    expected_begin = end;
                    let edge_kilometres = edge
                        .get("length")
                        .and_then(Value::as_f64)
                        .filter(|length| length.is_finite() && *length > 0.0)
                        .ok_or(RoutingError::InvalidProviderResponse)?;
                    traced_kilometres += edge_kilometres;
                    let distance = kilometres_to_metres(edge_kilometres)
                        .ok_or(RoutingError::InvalidProviderResponse)?;
                    edges.push(RouteEdge {
                        edge_id: id.to_string(),
                        from: None,
                        to: None,
                        begin_geometry_index: begin,
                        end_geometry_index: end,
                        direction: TraversalDirection::Forward,
                        distance: Some(distance),
                        qualifications: vec![Qualification::ProviderAccessUnverified],
                    });
                }
                if edges.is_empty() || expected_begin != geometry.len() - 1 {
                    return Err(RoutingError::InvalidProviderResponse);
                }
                let summary_kilometres = trip
                    .get("summary")
                    .and_then(|summary| summary.get("length"))
                    .and_then(Value::as_f64)
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                let distance = kilometres_to_metres(summary_kilometres)
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                // Each provider kilometre value is rounded to 0.001 km.
                let rounding_tolerance = 0.0005 * (edges.len() + 1) as f64;
                if (traced_kilometres - summary_kilometres).abs()
                    > rounding_tolerance + f64::EPSILON
                {
                    return Err(RoutingError::InvalidProviderResponse);
                }
                let raw_manoeuvres = leg
                    .get("maneuvers")
                    .and_then(Value::as_array)
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                if raw_manoeuvres.is_empty() {
                    return Err(RoutingError::InvalidProviderResponse);
                }
                let mut previous_geometry_index = 0_usize;
                let manoeuvres = raw_manoeuvres
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let geometry_index = json_usize(item, "begin_shape_index")?;
                        if geometry_index >= geometry.len()
                            || (index == 0 && geometry_index != 0)
                            || (index > 0 && geometry_index < previous_geometry_index)
                            || (index + 1 == raw_manoeuvres.len()
                                && geometry_index != geometry.len() - 1)
                        {
                            return Err(RoutingError::InvalidProviderResponse);
                        }
                        previous_geometry_index = geometry_index;
                        let provider_type = item
                            .get("type")
                            .and_then(Value::as_u64)
                            .and_then(|value| u8::try_from(value).ok())
                            .ok_or(RoutingError::InvalidProviderResponse)?;
                        let kind = if index == 0 {
                            ManoeuvreKind::Depart
                        } else if index + 1 == raw_manoeuvres.len() {
                            ManoeuvreKind::Arrive
                        } else {
                            ManoeuvreKind::Provider(provider_type)
                        };
                        Ok(Manoeuvre {
                            kind,
                            geometry_index,
                        })
                    })
                    .collect::<Result<_, RoutingError>>()?;
                Ok(RoutePath {
                    edges,
                    geometry,
                    distance,
                    manoeuvres,
                    provenance: RouteProvenance {
                        provider: "valhalla".into(),
                        data_version: self.data_version.clone(),
                        attribution: Some(self.attribution.clone()),
                    },
                    legality: RouteLegalityEvidence {
                        policy_version: VALHALLA_UNVERIFIED_POLICY_VERSION.into(),
                        access_verified: false,
                        direction_verified: false,
                        turns_verified: false,
                    },
                })
            })
            .await
    }
}

fn json_usize(value: &Value, key: &str) -> Result<usize, RoutingError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(RoutingError::InvalidProviderResponse)
}

fn kilometres_to_metres(kilometres: f64) -> Option<Metres> {
    if !kilometres.is_finite() || kilometres <= 0.0 {
        return None;
    }
    let metres = (kilometres * 1_000.0).round();
    if metres > u32::MAX as f64 {
        return None;
    }
    Metres::new(metres as u32).ok()
}

fn decode_polyline6(encoded: &str) -> Result<Vec<GeoPointE7>, RoutingError> {
    let bytes = encoded.as_bytes();
    let mut index = 0;
    let mut latitude = 0_i64;
    let mut longitude = 0_i64;
    let mut points = Vec::new();
    while index < bytes.len() {
        latitude = latitude
            .checked_add(decode_polyline_component(bytes, &mut index)?)
            .ok_or(RoutingError::InvalidProviderResponse)?;
        longitude = longitude
            .checked_add(decode_polyline_component(bytes, &mut index)?)
            .ok_or(RoutingError::InvalidProviderResponse)?;
        let latitude_e7 = i32::try_from(
            latitude
                .checked_mul(10)
                .ok_or(RoutingError::InvalidProviderResponse)?,
        )
        .map_err(|_| RoutingError::InvalidProviderResponse)?;
        let longitude_e7 = i32::try_from(
            longitude
                .checked_mul(10)
                .ok_or(RoutingError::InvalidProviderResponse)?,
        )
        .map_err(|_| RoutingError::InvalidProviderResponse)?;
        points.push(GeoPointE7::new(latitude_e7, longitude_e7));
    }
    if points.len() < 2 {
        return Err(RoutingError::InvalidProviderResponse);
    }
    Ok(points)
}

fn decode_polyline_component(bytes: &[u8], index: &mut usize) -> Result<i64, RoutingError> {
    let mut result = 0_u64;
    let mut shift = 0_u32;
    loop {
        let byte = *bytes
            .get(*index)
            .ok_or(RoutingError::InvalidProviderResponse)?;
        *index += 1;
        let value = byte
            .checked_sub(63)
            .ok_or(RoutingError::InvalidProviderResponse)?;
        result |= u64::from(value & 0x1f) << shift;
        if value < 0x20 {
            break;
        }
        shift += 5;
        if shift > 60 {
            return Err(RoutingError::InvalidProviderResponse);
        }
    }
    let signed = if result & 1 == 1 {
        !(result >> 1) as i64
    } else {
        (result >> 1) as i64
    };
    Ok(signed)
}

#[async_trait]
pub trait RouteProvider: Send + Sync {
    async fn route(
        &self,
        query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError>;
}

#[async_trait]
impl RouteProvider for ControlledGraph {
    async fn route(
        &self,
        query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        let query = self.rewrite_point_query(query)?;
        control.run(async { self.calculate(&query) }).await
    }
}

#[async_trait]
impl RouteProvider for LocalOsmGraph {
    async fn route(
        &self,
        query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        let query = self.rewrite_query(query)?;
        let mut route = control.run(async { self.graph.calculate(&query) }).await?;
        route.provenance = RouteProvenance {
            provider: "local-osm-snapshot".into(),
            data_version: Some(self.data_version.clone()),
            attribution: Some(self.attribution.clone()),
        };
        route.legality = RouteLegalityEvidence {
            policy_version: OSM_PEDESTRIAN_POLICY_VERSION.into(),
            access_verified: true,
            direction_verified: true,
            turns_verified: true,
        };
        Ok(route)
    }
}

pub async fn route_with_retries<P: RouteProvider>(
    provider: &P,
    query: &RouteQuery,
    control: &AttemptControl,
    max_attempts: usize,
) -> Result<RoutePath, RoutingError> {
    let mut last_error = RoutingError::Unavailable;
    for _ in 0..max_attempts {
        match provider.route(query, control).await {
            Err(RoutingError::Unavailable) => last_error = RoutingError::Unavailable,
            result => return result,
        }
    }
    if control.attempts_remaining.load(Ordering::Acquire) == 0 {
        Err(RoutingError::BudgetExhausted)
    } else {
        Err(last_error)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RoutingError {
    #[error("routing provider is unavailable")]
    Unavailable,
    #[error("routing capability is unsupported")]
    Unsupported,
    #[error("no legal path connects the ordered anchors")]
    NoPath,
    #[error("routing request is invalid")]
    InvalidRequest,
    #[error("controlled graph fixture is invalid")]
    InvalidGraph,
    #[error("routing snapshot checksum does not match its manifest")]
    SnapshotChecksumMismatch,
    #[error("routing snapshot is invalid or unsupported")]
    InvalidSnapshot,
    #[error("routing was cancelled")]
    Cancelled,
    #[error("routing deadline was exceeded")]
    DeadlineExceeded,
    #[error("routing attempt budget was exhausted")]
    BudgetExhausted,
    #[error("route distance exceeds canonical metre capacity")]
    DistanceOverflow,
    #[error("routing provider returned an invalid response")]
    InvalidProviderResponse,
}
