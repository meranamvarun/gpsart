use async_trait::async_trait;
use gpsart_domain::{AccessPolicy, ActivityProfileDefinition, Metres, RouteRequestProfile};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteQuery {
    pub profile: RouteRequestProfile,
    pub target_distance: Metres,
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

#[async_trait]
pub trait RouteProvider: Send + Sync {
    type Output: Send;
    async fn route(&self, query: &RouteQuery) -> Result<Self::Output, RoutingError>;
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RoutingError {
    #[error("routing provider is unavailable")]
    Unavailable,
    #[error("routing capability is unsupported")]
    Unsupported,
}
