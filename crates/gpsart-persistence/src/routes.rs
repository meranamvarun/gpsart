use async_trait::async_trait;
use gpsart_domain::profile_for;
use gpsart_routing::{CandidatePart, ManoeuvreKind, ScoredCandidate};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgPoolOptions};
use std::time::SystemTime;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

pub const DISTANCE_TOLERANCE_METRES: u32 = 1;
pub const SNAPSHOT_VERSION: &str = "route-candidate-snapshot-v1";
pub const LEGALITY_VERSION: &str = "candidate-legality-v1";
pub const ROUTE_USE_ADVISORY_VERSION: &str = "route-use-advisory-v1";
pub const REQUEST_VERSION: &str = "route-request-v1";
pub const ALGORITHM_VERSION: &str = "algorithm-v1";

#[derive(Clone, Debug, PartialEq)]
pub struct GeoJsonLineString {
    pub coordinates: Vec<[f64; 2]>,
}

impl Serialize for GeoJsonLineString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct GeoJson<'a> {
            #[serde(rename = "type")]
            kind: &'static str,
            coordinates: &'a [[f64; 2]],
        }
        GeoJson {
            kind: "LineString",
            coordinates: &self.coordinates,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GeoJsonLineString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct GeoJson {
            #[serde(rename = "type")]
            kind: String,
            coordinates: Vec<[f64; 2]>,
        }
        let value = GeoJson::deserialize(deserializer)?;
        if value.kind != "LineString" {
            return Err(serde::de::Error::custom("expected a GeoJSON LineString"));
        }
        Ok(Self {
            coordinates: value.coordinates,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalClass {
    Artwork,
    Connector,
    Access,
    Return,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraversalDirection {
    Forward,
    Reverse,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteTraversal {
    pub edge_id: String,
    pub class: TraversalClass,
    pub direction: TraversalDirection,
    pub begin_geometry_index: usize,
    pub end_geometry_index: usize,
    pub distance_metres: u32,
    pub retraced: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DistanceSummary {
    pub artwork_metres: u32,
    pub connector_metres: u32,
    pub total_metres: u32,
    pub repeated_metres: u32,
    pub repeated_included_in_total: bool,
    pub equality_tolerance_metres: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityBand {
    StrongMatch,
    ApproximateMatch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateQualifications {
    pub legality_version: String,
    pub route_use_advisory_version: String,
    pub access_verified: bool,
    pub direction_verified: bool,
    pub turns_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotVersions {
    pub snapshot: String,
    pub request: String,
    pub profile: String,
    pub template_id: String,
    pub template: u32,
    pub template_hash: String,
    pub algorithm: String,
    pub routing_data: String,
    pub routing_policy: String,
    pub candidate: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateProvenance {
    pub routing_provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstructionKind {
    Depart,
    Via,
    Arrive,
    Provider,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteInstruction {
    pub kind: InstructionKind,
    pub geometry_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_code: Option<u8>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSnapshot {
    pub id: Uuid,
    pub generation_job_id: Uuid,
    pub candidate_version: u32,
    pub request_revision: u32,
    pub geometry: GeoJsonLineString,
    pub traversals: Vec<RouteTraversal>,
    pub instructions: Vec<RouteInstruction>,
    pub duration_seconds: u64,
    pub distances: DistanceSummary,
    pub quality_band: QualityBand,
    pub qualifications: CandidateQualifications,
    pub versions: SnapshotVersions,
    pub generated_at_epoch_seconds: u64,
    pub provenance: CandidateProvenance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotIdentity {
    pub id: Uuid,
    pub generation_job_id: Uuid,
    pub candidate_version: u32,
    pub request_revision: u32,
    pub generated_at_epoch_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DirectionMarker {
    pub traversal_index: usize,
    pub coordinate: [f64; 2],
    pub direction: TraversalDirection,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SnapshotError {
    #[error("snapshot identity or immutable version pins are invalid")]
    InvalidVersion,
    #[error("ordered route geometry is invalid")]
    InvalidGeometry,
    #[error("route traversal does not continuously classify the ordered geometry")]
    InvalidTraversal,
    #[error("route distance fields contradict the classified traversal")]
    DistanceMismatch,
    #[error("repeated distance must be explicitly included in total distance")]
    RepeatedDistanceNotIncluded,
    #[error("snapshot provenance or qualifications are incomplete")]
    IncompleteEvidence,
}

impl CandidateSnapshot {
    pub fn from_scored(
        source: &ScoredCandidate,
        identity: SnapshotIdentity,
    ) -> Result<Self, SnapshotError> {
        let candidate = source.candidate.candidate();
        let legality = source.candidate.legality_evidence();
        let mut artwork_metres = 0_u32;
        let mut connector_metres = 0_u32;
        let traversals = candidate
            .occurrences
            .iter()
            .map(|occurrence| {
                if occurrence.part == CandidatePart::Artwork {
                    artwork_metres = artwork_metres
                        .checked_add(occurrence.distance.value())
                        .ok_or(SnapshotError::DistanceMismatch)?;
                } else {
                    connector_metres = connector_metres
                        .checked_add(occurrence.distance.value())
                        .ok_or(SnapshotError::DistanceMismatch)?;
                }
                Ok(RouteTraversal {
                    edge_id: occurrence.edge_id.clone(),
                    class: match occurrence.part {
                        CandidatePart::Artwork => TraversalClass::Artwork,
                        CandidatePart::Connector => TraversalClass::Connector,
                        CandidatePart::Access => TraversalClass::Access,
                        CandidatePart::Return => TraversalClass::Return,
                    },
                    direction: match occurrence.direction {
                        gpsart_routing::TraversalDirection::Forward => TraversalDirection::Forward,
                        gpsart_routing::TraversalDirection::Reverse => TraversalDirection::Reverse,
                    },
                    begin_geometry_index: occurrence.begin_geometry_index,
                    end_geometry_index: occurrence.end_geometry_index,
                    distance_metres: occurrence.distance.value(),
                    retraced: occurrence.retraced,
                })
            })
            .collect::<Result<Vec<_>, SnapshotError>>()?;
        let instructions = candidate
            .manoeuvres
            .iter()
            .map(|manoeuvre| RouteInstruction {
                kind: match manoeuvre.kind {
                    ManoeuvreKind::Depart => InstructionKind::Depart,
                    ManoeuvreKind::Via => InstructionKind::Via,
                    ManoeuvreKind::Arrive => InstructionKind::Arrive,
                    ManoeuvreKind::Provider(_) => InstructionKind::Provider,
                },
                geometry_index: manoeuvre.geometry_index,
                provider_code: match manoeuvre.kind {
                    ManoeuvreKind::Provider(code) => Some(code),
                    _ => None,
                },
            })
            .collect();
        let snapshot = Self {
            id: identity.id,
            generation_job_id: identity.generation_job_id,
            candidate_version: identity.candidate_version,
            request_revision: identity.request_revision,
            geometry: GeoJsonLineString {
                coordinates: candidate
                    .geometry
                    .iter()
                    .map(|point| {
                        [
                            f64::from(point.longitude_e7) / 10_000_000.0,
                            f64::from(point.latitude_e7) / 10_000_000.0,
                        ]
                    })
                    .collect(),
            },
            traversals,
            instructions,
            duration_seconds: profile_for(candidate.profile.activity())
                .estimated_duration(candidate.distance)
                .as_secs(),
            distances: DistanceSummary {
                artwork_metres,
                connector_metres,
                total_metres: candidate.distance.value(),
                repeated_metres: candidate.repeated_distance_metres,
                repeated_included_in_total: true,
                equality_tolerance_metres: DISTANCE_TOLERANCE_METRES,
            },
            quality_band: match source.quality_band {
                gpsart_routing::QualityBand::StrongMatch => QualityBand::StrongMatch,
                gpsart_routing::QualityBand::ApproximateMatch => QualityBand::ApproximateMatch,
            },
            qualifications: CandidateQualifications {
                legality_version: legality.validation_version.clone(),
                route_use_advisory_version: legality.advisory_version.clone(),
                access_verified: legality.access_verified,
                direction_verified: legality.direction_verified,
                turns_verified: legality.turns_verified,
            },
            versions: SnapshotVersions {
                snapshot: SNAPSHOT_VERSION.into(),
                request: REQUEST_VERSION.into(),
                profile: legality.profile_version.clone(),
                template_id: candidate.template_id.clone(),
                template: candidate.template_version,
                template_hash: candidate.template_hash.clone(),
                algorithm: ALGORITHM_VERSION.into(),
                routing_data: legality.routing_data_version.clone(),
                routing_policy: legality.routing_policy_version.clone(),
                candidate: identity.candidate_version,
            },
            generated_at_epoch_seconds: identity.generated_at_epoch_seconds,
            provenance: CandidateProvenance {
                routing_provider: candidate.provenance.provider.clone(),
                attribution: candidate.provenance.attribution.clone(),
            },
        };
        snapshot.validate_source(source)?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<(), SnapshotError> {
        if self.id.is_nil()
            || self.generation_job_id.is_nil()
            || self.candidate_version == 0
            || self.request_revision == 0
            || self.generated_at_epoch_seconds == 0
            || self.versions.snapshot != SNAPSHOT_VERSION
            || self.versions.request != REQUEST_VERSION
            || self.versions.algorithm != ALGORITHM_VERSION
            || !matches!(
                self.versions.profile.as_str(),
                "walking-v1" | "running-v1" | "cycling-v1"
            )
            || self.versions.candidate != self.candidate_version
            || self.versions.template == 0
            || self.versions.template_hash.len() != 64
            || !self
                .versions
                .template_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || [
                &self.versions.request,
                &self.versions.profile,
                &self.versions.template_id,
                &self.versions.algorithm,
                &self.versions.routing_data,
                &self.versions.routing_policy,
            ]
            .iter()
            .any(|value| value.is_empty())
        {
            return Err(SnapshotError::InvalidVersion);
        }
        if self.geometry.coordinates.len() < 2
            || self
                .geometry
                .coordinates
                .iter()
                .any(|[longitude, latitude]| {
                    !longitude.is_finite()
                        || !latitude.is_finite()
                        || !(-180.0..=180.0).contains(longitude)
                        || !(-90.0..=90.0).contains(latitude)
                })
        {
            return Err(SnapshotError::InvalidGeometry);
        }
        if !self.distances.repeated_included_in_total {
            return Err(SnapshotError::RepeatedDistanceNotIncluded);
        }
        if self.distances.equality_tolerance_metres != DISTANCE_TOLERANCE_METRES {
            return Err(SnapshotError::DistanceMismatch);
        }
        let mut expected_index = 0_usize;
        let mut total = 0_u32;
        let mut artwork = 0_u32;
        let mut connector = 0_u32;
        let mut repeated = 0_u32;
        for traversal in &self.traversals {
            if traversal.edge_id.is_empty()
                || traversal.distance_metres == 0
                || traversal.begin_geometry_index != expected_index
                || traversal.begin_geometry_index >= traversal.end_geometry_index
                || traversal.end_geometry_index >= self.geometry.coordinates.len()
            {
                return Err(SnapshotError::InvalidTraversal);
            }
            expected_index = traversal.end_geometry_index;
            total = total
                .checked_add(traversal.distance_metres)
                .ok_or(SnapshotError::DistanceMismatch)?;
            if traversal.class == TraversalClass::Artwork {
                artwork = artwork
                    .checked_add(traversal.distance_metres)
                    .ok_or(SnapshotError::DistanceMismatch)?;
            } else {
                connector = connector
                    .checked_add(traversal.distance_metres)
                    .ok_or(SnapshotError::DistanceMismatch)?;
            }
            if traversal.retraced {
                repeated = repeated
                    .checked_add(traversal.distance_metres)
                    .ok_or(SnapshotError::DistanceMismatch)?;
            }
        }
        if self.traversals.is_empty()
            || expected_index != self.geometry.coordinates.len() - 1
            || total.abs_diff(self.distances.total_metres) > DISTANCE_TOLERANCE_METRES
            || artwork.abs_diff(self.distances.artwork_metres) > DISTANCE_TOLERANCE_METRES
            || connector.abs_diff(self.distances.connector_metres) > DISTANCE_TOLERANCE_METRES
            || artwork.checked_add(connector).is_none_or(|sum| {
                sum.abs_diff(self.distances.total_metres) > DISTANCE_TOLERANCE_METRES
            })
            || repeated.abs_diff(self.distances.repeated_metres) > DISTANCE_TOLERANCE_METRES
            || self.distances.repeated_metres > self.distances.total_metres
        {
            return Err(SnapshotError::DistanceMismatch);
        }
        if self.qualifications.legality_version != LEGALITY_VERSION
            || self.qualifications.route_use_advisory_version != ROUTE_USE_ADVISORY_VERSION
            || !self.qualifications.access_verified
            || !self.qualifications.direction_verified
            || !self.qualifications.turns_verified
            || self.provenance.routing_provider.is_empty()
            || self
                .provenance
                .attribution
                .as_ref()
                .is_some_and(String::is_empty)
        {
            return Err(SnapshotError::IncompleteEvidence);
        }
        if self.duration_seconds == 0
            || self.instructions.len() < 2
            || self.instructions.first().is_none_or(|instruction| {
                instruction.kind != InstructionKind::Depart || instruction.geometry_index != 0
            })
            || self.instructions.last().is_none_or(|instruction| {
                instruction.kind != InstructionKind::Arrive
                    || instruction.geometry_index != self.geometry.coordinates.len() - 1
            })
            || self.instructions.iter().any(|instruction| {
                instruction.geometry_index >= self.geometry.coordinates.len()
                    || (instruction.kind == InstructionKind::Provider)
                        != instruction.provider_code.is_some()
            })
            || self
                .instructions
                .windows(2)
                .any(|pair| pair[0].geometry_index > pair[1].geometry_index)
        {
            return Err(SnapshotError::InvalidTraversal);
        }
        Ok(())
    }

    pub fn start(&self) -> [f64; 2] {
        self.geometry.coordinates[0]
    }

    pub fn finish(&self) -> [f64; 2] {
        self.geometry.coordinates[self.geometry.coordinates.len() - 1]
    }

    pub fn direction_markers(&self) -> Vec<DirectionMarker> {
        self.traversals
            .iter()
            .enumerate()
            .map(|(traversal_index, traversal)| DirectionMarker {
                traversal_index,
                coordinate: self.geometry.coordinates[traversal.begin_geometry_index],
                direction: traversal.direction,
            })
            .collect()
    }

    pub fn validate_source(&self, source: &ScoredCandidate) -> Result<(), SnapshotError> {
        self.validate()?;
        let candidate = source.candidate.candidate();
        let legality = source.candidate.legality_evidence();
        if !legality.access_verified
            || !legality.direction_verified
            || !legality.turns_verified
            || self.qualifications.legality_version != legality.validation_version
            || self.qualifications.route_use_advisory_version != legality.advisory_version
            || self.qualifications.access_verified != legality.access_verified
            || self.qualifications.direction_verified != legality.direction_verified
            || self.qualifications.turns_verified != legality.turns_verified
            || self.versions.profile != legality.profile_version
            || self.versions.routing_data != legality.routing_data_version
            || self.versions.routing_policy != legality.routing_policy_version
            || self.versions.profile != candidate.profile.version().as_str()
            || self.versions.template_id != candidate.template_id
            || self.versions.template != candidate.template_version
            || self.versions.template_hash != candidate.template_hash
            || self.provenance.routing_provider != candidate.provenance.provider
            || self.provenance.attribution != candidate.provenance.attribution
            || candidate.provenance.data_version.as_deref()
                != Some(self.versions.routing_data.as_str())
            || self.distances.total_metres != candidate.distance.value()
            || self.distances.repeated_metres != candidate.repeated_distance_metres
            || !matches!(
                (self.quality_band, source.quality_band),
                (
                    QualityBand::StrongMatch,
                    gpsart_routing::QualityBand::StrongMatch
                ) | (
                    QualityBand::ApproximateMatch,
                    gpsart_routing::QualityBand::ApproximateMatch
                )
            )
            || self.geometry.coordinates.len() != candidate.geometry.len()
            || self.traversals.len() != candidate.occurrences.len()
            || self.instructions.len() != candidate.manoeuvres.len()
            || self.duration_seconds
                != profile_for(candidate.profile.activity())
                    .estimated_duration(candidate.distance)
                    .as_secs()
        {
            return Err(SnapshotError::IncompleteEvidence);
        }
        for (coordinate, point) in self.geometry.coordinates.iter().zip(&candidate.geometry) {
            if coordinate
                != &[
                    f64::from(point.longitude_e7) / 10_000_000.0,
                    f64::from(point.latitude_e7) / 10_000_000.0,
                ]
            {
                return Err(SnapshotError::InvalidGeometry);
            }
        }
        for (traversal, occurrence) in self.traversals.iter().zip(&candidate.occurrences) {
            let class_matches = matches!(
                (traversal.class, occurrence.part),
                (TraversalClass::Artwork, CandidatePart::Artwork)
                    | (TraversalClass::Connector, CandidatePart::Connector)
                    | (TraversalClass::Access, CandidatePart::Access)
                    | (TraversalClass::Return, CandidatePart::Return)
            );
            let direction_matches = matches!(
                (traversal.direction, occurrence.direction),
                (
                    TraversalDirection::Forward,
                    gpsart_routing::TraversalDirection::Forward
                ) | (
                    TraversalDirection::Reverse,
                    gpsart_routing::TraversalDirection::Reverse
                )
            );
            if !class_matches
                || !direction_matches
                || traversal.edge_id != occurrence.edge_id
                || traversal.retraced != occurrence.retraced
                || traversal.begin_geometry_index != occurrence.begin_geometry_index
                || traversal.end_geometry_index != occurrence.end_geometry_index
                || traversal.distance_metres != occurrence.distance.value()
            {
                return Err(SnapshotError::InvalidTraversal);
            }
        }
        for (instruction, manoeuvre) in self.instructions.iter().zip(&candidate.manoeuvres) {
            let kind_matches = matches!(
                (instruction.kind, manoeuvre.kind),
                (InstructionKind::Depart, ManoeuvreKind::Depart)
                    | (InstructionKind::Via, ManoeuvreKind::Via)
                    | (InstructionKind::Arrive, ManoeuvreKind::Arrive)
            ) || matches!(
                (instruction.kind, instruction.provider_code, manoeuvre.kind),
                (
                    InstructionKind::Provider,
                    Some(left),
                    ManoeuvreKind::Provider(right)
                ) if left == right
            );
            if !kind_matches || instruction.geometry_index != manoeuvre.geometry_index {
                return Err(SnapshotError::InvalidTraversal);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RouteSnapshotError {
    #[error("route snapshot was not found")]
    NotFound,
    #[error("route snapshot storage is unavailable")]
    Unavailable,
    #[error("route snapshot failed immutable validation")]
    InvalidSnapshot,
}

#[async_trait]
pub trait RouteSnapshotStore: Send + Sync + 'static {
    async fn get(
        &self,
        owner: Uuid,
        route_id: Uuid,
    ) -> Result<CandidateSnapshot, RouteSnapshotError>;
}

#[derive(Clone)]
pub struct PostgresRouteSnapshotStore {
    pool: PgPool,
}

impl PostgresRouteSnapshotStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str) -> Result<Self, RouteSnapshotError> {
        PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map(Self::new)
            .map_err(|_| RouteSnapshotError::Unavailable)
    }

    pub async fn publish(
        &self,
        owner: Uuid,
        snapshot: &CandidateSnapshot,
        source: &ScoredCandidate,
        expires_at: SystemTime,
    ) -> Result<(), RouteSnapshotError> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|_| RouteSnapshotError::Unavailable)?;
        insert_validated_snapshot(&mut transaction, owner, snapshot, source, expires_at).await?;
        transaction
            .commit()
            .await
            .map_err(|_| RouteSnapshotError::Unavailable)
    }
}

pub(crate) async fn insert_validated_snapshot(
    transaction: &mut Transaction<'_, Postgres>,
    owner: Uuid,
    snapshot: &CandidateSnapshot,
    source: &ScoredCandidate,
    expires_at: SystemTime,
) -> Result<(), RouteSnapshotError> {
    snapshot
        .validate_source(source)
        .map_err(|_| RouteSnapshotError::InvalidSnapshot)?;
    let created_at = SystemTime::UNIX_EPOCH
        .checked_add(std::time::Duration::from_secs(
            snapshot.generated_at_epoch_seconds,
        ))
        .ok_or(RouteSnapshotError::InvalidSnapshot)?;
    if expires_at <= created_at {
        return Err(RouteSnapshotError::InvalidSnapshot);
    }
    let start = snapshot.start();
    let finish = snapshot.finish();
    let geometry_json = serde_json::to_string(&snapshot.geometry)
        .map_err(|_| RouteSnapshotError::InvalidSnapshot)?;
    let snapshot_json =
        serde_json::to_string(snapshot).map_err(|_| RouteSnapshotError::InvalidSnapshot)?;
    let request_revision = i32::try_from(snapshot.request_revision)
        .map_err(|_| RouteSnapshotError::InvalidSnapshot)?;
    let candidate_version = i32::try_from(snapshot.candidate_version)
        .map_err(|_| RouteSnapshotError::InvalidSnapshot)?;
    let inserted = sqlx::query(
            "INSERT INTO route_candidates (id, guest_session_id, generation_job_id, candidate_version, start_point, end_point, canonical_route, distance_metres, map_data_version, algorithm_version, created_at, expires_at) SELECT $1,$2,gj.id,$4,ST_SetSRID(ST_MakePoint($5,$6),4326),ST_SetSRID(ST_MakePoint($7,$8),4326),ST_SetSRID(ST_GeomFromGeoJSON($9),4326),$10,$11,$12,$13,$14 FROM generation_jobs gj JOIN planning_requests pr ON pr.id=gj.planning_request_id WHERE gj.id=$3 AND gj.guest_session_id=$2 AND pr.revision=$15 AND pr.activity_profile_version=$16 AND pr.template_id=$17 AND pr.template_version=$18 AND pr.map_data_version=$11 AND pr.algorithm_version=$12 AND ST_X(pr.requested_pin)=$5 AND ST_Y(pr.requested_pin)=$6 AND (NOT pr.return_to_start OR (ST_X(pr.requested_pin)=$7 AND ST_Y(pr.requested_pin)=$8)) AND $10 BETWEEN pr.requested_distance_metres - pr.distance_tolerance_metres AND pr.requested_distance_metres + pr.distance_tolerance_metres",
        )
        .bind(snapshot.id)
        .bind(owner)
        .bind(snapshot.generation_job_id)
        .bind(candidate_version)
        .bind(start[0])
        .bind(start[1])
        .bind(finish[0])
        .bind(finish[1])
        .bind(geometry_json)
        .bind(i32::try_from(snapshot.distances.total_metres).map_err(|_| RouteSnapshotError::InvalidSnapshot)?)
        .bind(&snapshot.versions.routing_data)
        .bind(&snapshot.versions.algorithm)
        .bind(OffsetDateTime::from(created_at))
        .bind(OffsetDateTime::from(expires_at))
        .bind(request_revision)
        .bind(&snapshot.versions.profile)
        .bind(&snapshot.versions.template_id)
        .bind(snapshot.versions.template.to_string())
        .execute(&mut **transaction)
        .await
        .map_err(|_| RouteSnapshotError::Unavailable)?
        .rows_affected();
    if inserted != 1 {
        return Err(RouteSnapshotError::InvalidSnapshot);
    }
    sqlx::query(
        "INSERT INTO route_candidate_snapshots (candidate_id, snapshot_json) VALUES ($1,$2::jsonb)",
    )
    .bind(snapshot.id)
    .bind(snapshot_json)
    .execute(&mut **transaction)
    .await
    .map_err(|_| RouteSnapshotError::Unavailable)?;
    Ok(())
}

#[async_trait]
impl RouteSnapshotStore for PostgresRouteSnapshotStore {
    async fn get(
        &self,
        owner: Uuid,
        route_id: Uuid,
    ) -> Result<CandidateSnapshot, RouteSnapshotError> {
        let row = sqlx::query(
            "SELECT snapshot_json::text AS snapshot_json FROM route_candidate_snapshots rcs JOIN route_candidates rc ON rc.id=rcs.candidate_id WHERE rc.id=$1 AND rc.guest_session_id=$2",
        )
        .bind(route_id)
        .bind(owner)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| RouteSnapshotError::Unavailable)?
        .ok_or(RouteSnapshotError::NotFound)?;
        let encoded: String = row
            .try_get("snapshot_json")
            .map_err(|_| RouteSnapshotError::Unavailable)?;
        let snapshot: CandidateSnapshot =
            serde_json::from_str(&encoded).map_err(|_| RouteSnapshotError::InvalidSnapshot)?;
        snapshot
            .validate()
            .map_err(|_| RouteSnapshotError::InvalidSnapshot)?;
        Ok(snapshot)
    }
}
