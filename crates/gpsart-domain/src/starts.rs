use crate::{ProfileVersion, RouteRequestProfile};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub const MAX_CONFIRMABLE_SNAP_METRES: u32 = 30;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub longitude: f64,
    pub latitude: f64,
}

impl Point {
    pub fn new(longitude: f64, latitude: f64) -> Result<Self, ResolveError> {
        if !longitude.is_finite()
            || !latitude.is_finite()
            || !(-180.0..=180.0).contains(&longitude)
            || !(-90.0..=90.0).contains(&latitude)
        {
            return Err(ResolveError::InvalidCoordinates);
        }
        Ok(Self {
            longitude,
            latitude,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Coverage {
    pub version: String,
    pub geojson: serde_json::Value,
    min_longitude: f64,
    min_latitude: f64,
    max_longitude: f64,
    max_latitude: f64,
}

impl Coverage {
    pub fn rectangle(
        version: impl Into<String>,
        min_longitude: f64,
        min_latitude: f64,
        max_longitude: f64,
        max_latitude: f64,
    ) -> Result<Self, ResolveError> {
        let lower = Point::new(min_longitude, min_latitude)?;
        let upper = Point::new(max_longitude, max_latitude)?;
        if lower.longitude >= upper.longitude || lower.latitude >= upper.latitude {
            return Err(ResolveError::InvalidCoordinates);
        }
        let geojson = serde_json::json!({
            "type": "Feature",
            "properties": {
                "coverage_policy_version": version.into(),
                "local_route_feasibility": "not_evaluated"
            },
            "geometry": {
                "type": "Polygon",
                "coordinates": [[
                    [min_longitude, min_latitude], [max_longitude, min_latitude],
                    [max_longitude, max_latitude], [min_longitude, max_latitude],
                    [min_longitude, min_latitude]
                ]]
            }
        });
        let version = geojson["properties"]["coverage_policy_version"]
            .as_str()
            .expect("coverage version was inserted as a string")
            .to_owned();
        Ok(Self {
            version,
            geojson,
            min_longitude,
            min_latitude,
            max_longitude,
            max_latitude,
        })
    }

    pub fn contains(&self, point: Point) -> bool {
        (self.min_longitude..=self.max_longitude).contains(&point.longitude)
            && (self.min_latitude..=self.max_latitude).contains(&point.latitude)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NetworkPoint {
    pub point: Point,
    pub snap_distance_metres: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderError;

pub trait StartResolverProvider: Send + Sync + 'static {
    fn nearest_routable(
        &self,
        requested: Point,
        profile: RouteRequestProfile,
    ) -> Result<Option<NetworkPoint>, ProviderError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolutionContext {
    pub guest_session_id: String,
    pub profile: RouteRequestProfile,
    pub coverage_policy_version: String,
    pub data_version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ResolutionId(Uuid);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StartResolution {
    pub id: ResolutionId,
    pub requested_point: Point,
    pub candidate_network_point: Point,
    pub snap_distance_metres: u32,
    pub profile_version: ProfileVersion,
    pub coverage_policy_version: String,
    pub data_version: String,
    pub expires_at_epoch_seconds: u64,
    pub confirmation_required: bool,
}

#[derive(Clone)]
struct StoredResolution {
    public: StartResolution,
    context: ResolutionContext,
    expires_at: SystemTime,
}

pub struct StartResolver<P> {
    coverage: Coverage,
    provider: P,
    ttl: Duration,
    resolutions: Mutex<HashMap<ResolutionId, StoredResolution>>,
}

impl<P: StartResolverProvider> StartResolver<P> {
    pub fn new(coverage: Coverage, provider: P, ttl: Duration) -> Self {
        Self {
            coverage,
            provider,
            ttl,
            resolutions: Mutex::new(HashMap::new()),
        }
    }

    pub fn coverage(&self) -> &Coverage {
        &self.coverage
    }

    pub fn resolve(
        &self,
        requested_point: Point,
        context: ResolutionContext,
        now: SystemTime,
    ) -> Result<StartResolution, ResolveError> {
        if context.coverage_policy_version != self.coverage.version {
            return Err(ResolveError::CoveragePolicyChanged);
        }
        if !self.coverage.contains(requested_point) {
            return Err(ResolveError::UnsupportedCoverage);
        }
        let candidate = self
            .provider
            .nearest_routable(requested_point, context.profile)
            .map_err(|_| ResolveError::ProviderFailure)?
            .ok_or(ResolveError::NoRoutableAccess)?;
        let expires_at = now
            .checked_add(self.ttl)
            .ok_or(ResolveError::ProviderFailure)?;
        let public = StartResolution {
            id: ResolutionId(Uuid::new_v4()),
            requested_point,
            candidate_network_point: candidate.point,
            snap_distance_metres: candidate.snap_distance_metres,
            profile_version: context.profile.version(),
            coverage_policy_version: context.coverage_policy_version.clone(),
            data_version: context.data_version.clone(),
            expires_at_epoch_seconds: expires_at
                .duration_since(UNIX_EPOCH)
                .map_err(|_| ResolveError::ProviderFailure)?
                .as_secs(),
            confirmation_required: true,
        };
        self.resolutions
            .lock()
            .expect("resolution lock poisoned")
            .insert(
                public.id,
                StoredResolution {
                    public: public.clone(),
                    context,
                    expires_at,
                },
            );
        Ok(public)
    }

    pub fn confirm(
        &self,
        id: ResolutionId,
        context: &ResolutionContext,
        now: SystemTime,
    ) -> Result<Point, ResolveError> {
        let resolutions = self.resolutions.lock().expect("resolution lock poisoned");
        let stored = resolutions
            .get(&id)
            .ok_or(ResolveError::ExpiredResolution)?;
        if stored.context.guest_session_id != context.guest_session_id {
            return Err(ResolveError::WrongSession);
        }
        if now >= stored.expires_at {
            return Err(ResolveError::ExpiredResolution);
        }
        if stored.context.profile != context.profile {
            return Err(ResolveError::ProfileChanged);
        }
        if stored.context.coverage_policy_version != context.coverage_policy_version {
            return Err(ResolveError::CoveragePolicyChanged);
        }
        if stored.context.data_version != context.data_version {
            return Err(ResolveError::DataVersionChanged);
        }
        if stored.public.snap_distance_metres > MAX_CONFIRMABLE_SNAP_METRES {
            return Err(ResolveError::SnapTooFar);
        }
        Ok(stored.public.candidate_network_point)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveError {
    InvalidCoordinates,
    UnsupportedCoverage,
    NoRoutableAccess,
    ProviderFailure,
    ExpiredResolution,
    WrongSession,
    ProfileChanged,
    CoveragePolicyChanged,
    DataVersionChanged,
    SnapTooFar,
}

impl ResolveError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidCoordinates => "invalid_coordinates",
            Self::UnsupportedCoverage => "unsupported_coverage",
            Self::NoRoutableAccess => "no_routable_access",
            Self::ProviderFailure => "provider_failure",
            Self::ExpiredResolution => "expired_resolution",
            Self::WrongSession => "wrong_session",
            Self::ProfileChanged => "profile_changed",
            Self::CoveragePolicyChanged => "coverage_policy_changed",
            Self::DataVersionChanged => "data_version_changed",
            Self::SnapTooFar => "snap_too_far",
        }
    }
}
