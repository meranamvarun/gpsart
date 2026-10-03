use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

pub mod starts;
pub mod templates;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityProfile {
    Walking,
    Running,
    Cycling,
}

impl ActivityProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Walking => "walking",
            Self::Running => "running",
            Self::Cycling => "cycling",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ProfileVersion {
    #[serde(rename = "walking-v1")]
    WalkingV1,
    #[serde(rename = "running-v1")]
    RunningV1,
    #[serde(rename = "cycling-v1")]
    CyclingV1,
}

impl ProfileVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WalkingV1 => "walking-v1",
            Self::RunningV1 => "running-v1",
            Self::CyclingV1 => "cycling-v1",
        }
    }
}

impl fmt::Display for ProfileVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessPolicy {
    Foot,
    Cycling,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivityProfileDefinition {
    pub activity: ActivityProfile,
    pub version: ProfileVersion,
    pub access_policy: AccessPolicy,
    pub default_pace_seconds_per_km: u32,
}

impl ActivityProfileDefinition {
    pub const fn canonical_distance(self, distance: Metres) -> Metres {
        distance
    }

    pub const fn estimated_duration(self, distance: Metres) -> std::time::Duration {
        let seconds =
            (distance.value() as u64 * self.default_pace_seconds_per_km as u64).div_ceil(1_000);
        std::time::Duration::from_secs(seconds)
    }
}

pub const fn profile_for(activity: ActivityProfile) -> ActivityProfileDefinition {
    match activity {
        ActivityProfile::Walking => ActivityProfileDefinition {
            activity,
            version: ProfileVersion::WalkingV1,
            access_policy: AccessPolicy::Foot,
            default_pace_seconds_per_km: 720,
        },
        ActivityProfile::Running => ActivityProfileDefinition {
            activity,
            version: ProfileVersion::RunningV1,
            access_policy: AccessPolicy::Foot,
            default_pace_seconds_per_km: 360,
        },
        ActivityProfile::Cycling => ActivityProfileDefinition {
            activity,
            version: ProfileVersion::CyclingV1,
            access_policy: AccessPolicy::Cycling,
            default_pace_seconds_per_km: 240,
        },
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "ProfileIdentityWire")]
struct ProfileIdentity {
    activity: ActivityProfile,
    profile_version: ProfileVersion,
}

#[derive(Deserialize)]
struct ProfileIdentityWire {
    activity: ActivityProfile,
    profile_version: ProfileVersion,
}

impl TryFrom<ProfileIdentityWire> for ProfileIdentity {
    type Error = DomainError;

    fn try_from(value: ProfileIdentityWire) -> Result<Self, Self::Error> {
        if profile_for(value.activity).version != value.profile_version {
            return Err(DomainError::ProfileVersionMismatch);
        }
        Ok(Self {
            activity: value.activity,
            profile_version: value.profile_version,
        })
    }
}

impl ProfileIdentity {
    const fn new(activity: ActivityProfile) -> Self {
        Self {
            activity,
            profile_version: profile_for(activity).version,
        }
    }

    const fn activity(self) -> ActivityProfile {
        self.activity
    }

    const fn version(self) -> ProfileVersion {
        self.profile_version
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RouteRequestProfile(ProfileIdentity);

impl RouteRequestProfile {
    pub const fn new(activity: ActivityProfile) -> Self {
        Self(ProfileIdentity::new(activity))
    }

    pub const fn activity(self) -> ActivityProfile {
        self.0.activity()
    }

    pub const fn version(self) -> ProfileVersion {
        self.0.version()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RouteCandidateProfile(ProfileIdentity);

impl RouteCandidateProfile {
    pub const fn from_request(request: &RouteRequestProfile) -> Self {
        Self(request.0)
    }

    pub const fn activity(self) -> ActivityProfile {
        self.0.activity()
    }

    pub const fn version(self) -> ProfileVersion {
        self.0.version()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Metres(u32);

impl Metres {
    pub fn new(value: u32) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::ZeroDistance);
        }
        Ok(Self(value))
    }

    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlanningRequestId(Uuid);

impl PlanningRequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PlanningRequestId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for PlanningRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum DomainError {
    #[error("distance must be greater than zero metres")]
    ZeroDistance,
    #[error("profile version does not match activity")]
    ProfileVersionMismatch,
}
