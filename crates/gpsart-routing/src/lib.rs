use async_trait::async_trait;
use gpsart_domain::{
    AccessPolicy, ActivityProfileDefinition, Metres, RouteRequestProfile, profile_for,
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::Notify;

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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Both,
    ForwardOnly,
    ReverseOnly,
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

    match (profile.access_policy, edge.access) {
        (_, Metadata::Known(Access::Forbidden))
        | (AccessPolicy::Cycling, Metadata::Known(Access::FootOnly))
        | (AccessPolicy::Foot, Metadata::Known(Access::BicycleOnly)) => {
            return Eligibility::Ineligible(IneligibleReason::AccessForbidden);
        }
        _ => {}
    }

    match (edge.direction, traversal) {
        (Metadata::Unknown, _) => {
            return Eligibility::Ineligible(IneligibleReason::DirectionUnknown);
        }
        (Metadata::Known(Direction::ForwardOnly), TraversalDirection::Reverse)
        | (Metadata::Known(Direction::ReverseOnly), TraversalDirection::Forward) => {
            return Eligibility::Ineligible(IneligibleReason::DirectionForbidden);
        }
        _ => {}
    }

    let mut qualifications = Vec::new();
    if edge.access == Metadata::Unknown {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePath {
    pub edges: Vec<RouteEdge>,
    pub geometry: Vec<GeoPointE7>,
    pub distance: Metres,
    pub manoeuvres: Vec<Manoeuvre>,
    pub provenance: RouteProvenance,
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
                for edge in trace_edges {
                    let id = edge
                        .get("id")
                        .and_then(Value::as_u64)
                        .ok_or(RoutingError::InvalidProviderResponse)?;
                    let begin = json_usize(edge, "begin_shape_index")?;
                    let end = json_usize(edge, "end_shape_index")?;
                    if begin >= end || end >= geometry.len() {
                        return Err(RoutingError::InvalidProviderResponse);
                    }
                    let distance = edge
                        .get("length")
                        .and_then(Value::as_f64)
                        .and_then(kilometres_to_metres);
                    edges.push(RouteEdge {
                        edge_id: id.to_string(),
                        from: None,
                        to: None,
                        begin_geometry_index: begin,
                        end_geometry_index: end,
                        direction: TraversalDirection::Forward,
                        distance,
                        qualifications: vec![Qualification::ProviderAccessUnverified],
                    });
                }
                if edges.is_empty() {
                    return Err(RoutingError::InvalidProviderResponse);
                }
                let distance = trip
                    .get("summary")
                    .and_then(|summary| summary.get("length"))
                    .and_then(Value::as_f64)
                    .and_then(kilometres_to_metres)
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                let raw_manoeuvres = leg
                    .get("maneuvers")
                    .and_then(Value::as_array)
                    .ok_or(RoutingError::InvalidProviderResponse)?;
                let manoeuvres = raw_manoeuvres
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let geometry_index = json_usize(item, "begin_shape_index")?;
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
        control.run(async { self.calculate(query) }).await
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
