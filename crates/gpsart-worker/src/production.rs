use crate::{WorkerError, run_provider_call};
use async_trait::async_trait;
use gpsart_config::WorkloadLimits;
use gpsart_domain::{
    Metres, RouteRequestProfile,
    templates::{Category, embedded_catalogue},
};
use gpsart_observability::{Correlation, MetricEvent, MetricName};
use gpsart_persistence::jobs::{
    ClaimedJobInput, Completion, JobError, JobLease, JobOutcomeCode, JobStage,
    PostgresJobCoordinator, SUPPORTED_ALGORITHM_VERSION,
};
use gpsart_persistence::routes::{CandidateSnapshot, SnapshotIdentity};
use gpsart_routing::{
    AnchorMatch, AttemptControl, CandidateError, CandidateQualityConfig, CandidateSearch,
    GeoPointE7, HeartSearchConfig, HeartSearchRequest, LocalOsmGraph, MatchOutcome, RoutePath,
    RouteProvider, RouteQuery, RoutingError, score_candidate,
};
use std::{
    fs,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};
use uuid::Uuid;

const LEASE_FOR: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct ProductionWorker {
    worker_id: Uuid,
    jobs: PostgresJobCoordinator,
    graph: Arc<LocalOsmGraph>,
    coverage_policy_version: Arc<str>,
}

impl ProductionWorker {
    pub async fn connect(
        database_url: &str,
        graph_path: &Path,
        checksum_path: &Path,
        coverage_policy_version: &str,
        limits: WorkloadLimits,
    ) -> Result<Self, WorkerError> {
        if coverage_policy_version.trim().is_empty() {
            return Err(WorkerError::InvalidConfiguration);
        }
        let graph_bytes = fs::read(graph_path).map_err(|_| WorkerError::InvalidConfiguration)?;
        let checksum =
            fs::read_to_string(checksum_path).map_err(|_| WorkerError::InvalidConfiguration)?;
        let graph = LocalOsmGraph::from_json(&graph_bytes, checksum.trim())
            .map_err(|_| WorkerError::InvalidConfiguration)?;
        let jobs = PostgresJobCoordinator::connect_with_limits(database_url, limits)
            .await
            .map_err(|_| WorkerError::DependencyUnavailable)?;
        Ok(Self {
            worker_id: Uuid::new_v4(),
            jobs,
            graph: Arc::new(graph),
            coverage_policy_version: Arc::from(coverage_policy_version),
        })
    }

    pub async fn take_one(&self) -> Result<(), WorkerError> {
        let now = SystemTime::now();
        let Some(lease) = self
            .jobs
            .claim(self.worker_id, now, LEASE_FOR)
            .await
            .map_err(job_error)?
        else {
            return Ok(());
        };
        self.execute(lease).await
    }

    async fn execute(&self, lease: JobLease) -> Result<(), WorkerError> {
        let started = SystemTime::now();
        let input = match self.jobs.load_claimed_input(&lease, started).await {
            Ok(input) => input,
            Err(JobError::InvalidInput) => {
                return self
                    .finish(
                        &lease,
                        Completion::Failed {
                            code: JobOutcomeCode::AttemptsExhausted,
                        },
                    )
                    .await;
            }
            Err(JobError::LeaseLost | JobError::Terminal | JobError::NotFound) => return Ok(()),
            Err(error) => return Err(job_error(error)),
        };
        let prepared = match prepare(
            &input,
            self.graph.data_version(),
            &self.coverage_policy_version,
            started,
        ) {
            Ok(prepared) => prepared,
            Err(()) => {
                return self
                    .finish(
                        &lease,
                        Completion::Failed {
                            code: JobOutcomeCode::AttemptsExhausted,
                        },
                    )
                    .await;
            }
        };
        match self
            .jobs
            .advance_stage(&lease, SystemTime::now(), JobStage::FittingDesign)
            .await
        {
            Ok(_) => {}
            Err(JobError::LeaseLost | JobError::Terminal | JobError::NotFound) => return Ok(()),
            Err(error) => return Err(job_error(error)),
        }
        let stop_reason = Arc::new(AtomicU8::new(SearchStop::None as u8));
        let provider = DurableGraph {
            graph: self.graph.clone(),
            jobs: self.jobs.clone(),
            lease: lease.clone(),
            stop_reason: stop_reason.clone(),
        };
        let search = CandidateSearch::new(&provider, &provider);
        let control =
            AttemptControl::with_timeout(prepared.time_left, prepared.config.max_provider_calls);
        let monitor_control = control.clone();
        let search_started = Instant::now();
        let search_future = async {
            match prepared.category {
                Category::Shape => {
                    search
                        .heart_with_control(
                            &prepared.template,
                            &prepared.request,
                            &prepared.config,
                            &control,
                        )
                        .await
                }
                Category::Text => {
                    search
                        .curated_text_with_control(
                            &prepared.template,
                            &prepared.request,
                            &prepared.config,
                            &control,
                        )
                        .await
                }
            }
        };
        tokio::pin!(search_future);
        let mut renewal = tokio::time::interval(Duration::from_secs(1));
        renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        renewal.tick().await;
        let mut current_lease = lease.clone();
        let outcome = loop {
            tokio::select! {
                result = &mut search_future => break result,
                _ = renewal.tick() => {
                    match self.jobs.renew(&current_lease, SystemTime::now(), LEASE_FOR).await {
                        Ok(renewed) => current_lease = renewed,
                        Err(JobError::LeaseLost | JobError::Terminal | JobError::NotFound) => {
                            monitor_control.cancel();
                            return Ok(());
                        }
                        Err(error) => {
                            monitor_control.cancel();
                            return Err(job_error(error));
                        }
                    }
                }
            }
        };
        let elapsed = search_started.elapsed();
        let forced_completion = forced_completion(
            SearchStop::from_u8(stop_reason.load(Ordering::Acquire)),
            matches!(&outcome, Ok(MatchOutcome::NoMatch))
                && elapsed >= prepared.config.wall_time,
        )?;
        MetricEvent::duration(
            MetricName::SearchLatency,
            Correlation::for_job(lease.job_id).with_attempt(lease.attempt),
            elapsed,
            None,
        )
        .with_result_class(match &forced_completion {
            Some(Completion::TimedOut) => "timed_out",
            Some(Completion::NoMatch {
                code: JobOutcomeCode::ExhaustedBudget,
            }) => "exhausted_budget",
            Some(Completion::Failed {
                code: JobOutcomeCode::ProviderUnavailable,
            }) => "provider_unavailable",
            Some(_) => "failed",
            None => match &outcome {
                Ok(MatchOutcome::Match(_)) => "match",
                Ok(MatchOutcome::NoMatch) => "no_match",
                Err(
                    CandidateError::Cancelled | CandidateError::Routing(RoutingError::Cancelled),
                ) => "cancelled",
                Err(CandidateError::Routing(RoutingError::DeadlineExceeded)) => "timed_out",
                Err(CandidateError::Routing(RoutingError::Unavailable)) => "provider_unavailable",
                Err(CandidateError::Routing(RoutingError::BudgetExhausted)) => "exhausted_budget",
                Err(_) => "failed",
            },
        })
        .emit();
        if let Some(completion) = forced_completion {
            return self.finish(&lease, completion).await;
        }
        let completion = match outcome {
            Ok(MatchOutcome::NoMatch) => Completion::NoMatch {
                code: JobOutcomeCode::InsufficientVisualMatch,
            },
            Err(CandidateError::Cancelled | CandidateError::Routing(RoutingError::Cancelled)) => {
                return Ok(());
            }
            Err(CandidateError::Routing(RoutingError::DeadlineExceeded)) => Completion::TimedOut,
            Err(CandidateError::Routing(RoutingError::BudgetExhausted)) => Completion::NoMatch {
                code: JobOutcomeCode::ExhaustedBudget,
            },
            Err(CandidateError::Routing(RoutingError::Unavailable)) => Completion::Failed {
                code: JobOutcomeCode::ProviderUnavailable,
            },
            Err(_) => Completion::Failed {
                code: JobOutcomeCode::AttemptsExhausted,
            },
            Ok(MatchOutcome::Match(candidate)) => {
                match self
                    .jobs
                    .advance_stage(&lease, SystemTime::now(), JobStage::ComparingRoutes)
                    .await
                {
                    Ok(_) => {}
                    Err(JobError::LeaseLost | JobError::Terminal | JobError::NotFound) => {
                        return Ok(());
                    }
                    Err(error) => return Err(job_error(error)),
                }
                let scored = score_candidate(
                    *candidate,
                    &prepared.template,
                    &CandidateQualityConfig::v1(
                        prepared.target,
                        prepared.request.minimum_distance,
                        prepared.request.maximum_distance,
                    ),
                )
                .map_err(|_| WorkerError::InvalidJobInput)?;
                let now = SystemTime::now();
                let snapshot = CandidateSnapshot::from_scored(
                    &scored,
                    SnapshotIdentity {
                        id: Uuid::new_v4(),
                        generation_job_id: lease.job_id,
                        candidate_version: 1,
                        request_revision: input.request_revision,
                        generated_at_epoch_seconds: now
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map_err(|_| WorkerError::InvalidJobInput)?
                            .as_secs(),
                    },
                )
                .map_err(|_| WorkerError::InvalidJobInput)?;
                match self
                    .jobs
                    .publish_success(
                        &lease,
                        input.owner,
                        &snapshot,
                        &scored,
                        input.expires_at,
                        now,
                    )
                    .await
                {
                    Ok(_) | Err(JobError::Terminal | JobError::LeaseLost) => return Ok(()),
                    Err(error) => return Err(job_error(error)),
                }
            }
        };
        self.finish(&lease, completion).await
    }

    async fn finish(&self, lease: &JobLease, completion: Completion) -> Result<(), WorkerError> {
        match self
            .jobs
            .complete(lease, &completion, SystemTime::now())
            .await
        {
            Ok(_) | Err(JobError::Terminal | JobError::LeaseLost) => Ok(()),
            Err(error) => Err(job_error(error)),
        }
    }
}

struct Prepared {
    template: gpsart_domain::templates::VersionedTemplate,
    category: Category,
    request: HeartSearchRequest,
    config: HeartSearchConfig,
    target: Metres,
    time_left: Duration,
}

fn prepare(
    input: &ClaimedJobInput,
    graph_version: &str,
    coverage_policy_version: &str,
    now: SystemTime,
) -> Result<Prepared, ()> {
    if input.map_data_version != graph_version
        || input.coverage_policy_version != coverage_policy_version
        || input.algorithm_version != SUPPORTED_ALGORITHM_VERSION
        || !input.return_to_start
    {
        return Err(());
    }
    let profile: RouteRequestProfile =
        serde_json::from_str(&input.activity_profile_json).map_err(|_| ())?;
    if profile.version().as_str() != input.activity_profile_version {
        return Err(());
    }
    let catalogue = embedded_catalogue().map_err(|_| ())?;
    let template = catalogue
        .get(&input.template_id, input.template_version)
        .cloned()
        .ok_or(())?;
    let minimum = input
        .requested_distance_metres
        .checked_sub(input.distance_tolerance_metres)
        .filter(|value| *value > 0)
        .and_then(|value| Metres::new(value).ok())
        .ok_or(())?;
    let maximum = input
        .requested_distance_metres
        .checked_add(input.distance_tolerance_metres)
        .and_then(|value| Metres::new(value).ok())
        .ok_or(())?;
    let target = Metres::new(input.requested_distance_metres).map_err(|_| ())?;
    let latitude_e7 = degrees_to_e7(input.confirmed_latitude, 90.0)?;
    let longitude_e7 = degrees_to_e7(input.confirmed_longitude, 180.0)?;
    let time_left = input.deadline_at.duration_since(now).map_err(|_| ())?;
    let category = template.definition.category;
    Ok(Prepared {
        template,
        category,
        request: HeartSearchRequest {
            profile,
            confirmed_start: GeoPointE7::new(latitude_e7, longitude_e7),
            minimum_distance: minimum,
            maximum_distance: maximum,
        },
        config: calibrated_config(category, time_left),
        target,
        time_left,
    })
}

fn degrees_to_e7(value: f64, limit: f64) -> Result<i32, ()> {
    if !value.is_finite() || !(-limit..=limit).contains(&value) {
        return Err(());
    }
    let scaled = value * 10_000_000.0;
    if scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
        return Err(());
    }
    let rounded = scaled.round();
    if (scaled - rounded).abs() > f64::EPSILON * scaled.abs().max(1.0) {
        return Err(());
    }
    Ok(rounded as i32)
}

fn calibrated_config(category: Category, time_left: Duration) -> HeartSearchConfig {
    let wall_time = time_left.min(Duration::from_secs(10));
    match category {
        Category::Shape => HeartSearchConfig {
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
            wall_time,
            anchor_tolerance_metres: 15,
        },
        Category::Text => HeartSearchConfig {
            placement_offsets_metres: vec![(66, 24)],
            scales_metres: vec![160],
            rotations_degrees: vec![-3],
            max_placements: 1,
            max_scales: 1,
            max_rotations: 1,
            max_anchor_matches: 16,
            max_provider_calls: 1,
            max_retained_points: 10_000,
            wall_time,
            anchor_tolerance_metres: 12,
        },
    }
}

struct DurableGraph {
    graph: Arc<LocalOsmGraph>,
    jobs: PostgresJobCoordinator,
    lease: JobLease,
    stop_reason: Arc<AtomicU8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum SearchStop {
    None = 0,
    BudgetExhausted = 1,
    DeadlineExceeded = 2,
    JobStateUnavailable = 3,
    ProviderTaskFailed = 4,
}

impl SearchStop {
    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::BudgetExhausted,
            2 => Self::DeadlineExceeded,
            3 => Self::JobStateUnavailable,
            4 => Self::ProviderTaskFailed,
            _ => Self::None,
        }
    }
}

fn forced_completion(
    stop: SearchStop,
    search_wall_time_exhausted: bool,
) -> Result<Option<Completion>, WorkerError> {
    match stop {
        SearchStop::JobStateUnavailable => Err(WorkerError::JobStateUnavailable),
        SearchStop::BudgetExhausted => Ok(Some(Completion::NoMatch {
            code: JobOutcomeCode::ExhaustedBudget,
        })),
        SearchStop::DeadlineExceeded => Ok(Some(Completion::TimedOut)),
        SearchStop::ProviderTaskFailed => Ok(Some(Completion::Failed {
            code: JobOutcomeCode::ProviderUnavailable,
        })),
        SearchStop::None if search_wall_time_exhausted => Ok(Some(Completion::TimedOut)),
        SearchStop::None => Ok(None),
    }
}

impl AnchorMatch for DurableGraph {
    fn nearest_anchor(&self, target: GeoPointE7, tolerance_metres: u32) -> Option<GeoPointE7> {
        self.graph.nearest_anchor(target, tolerance_metres)
    }
}

#[async_trait]
impl RouteProvider for DurableGraph {
    async fn route(
        &self,
        query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        let graph = self.graph.clone();
        let query = query.clone();
        let control = control.clone();
        match run_provider_call(&self.jobs, &self.lease, Duration::from_millis(1), || {
            tokio::spawn(async move { graph.route(&query, &control).await })
        })
        .await
        {
            Ok(Some(Ok(path))) => Ok(path),
            Ok(Some(Err(error))) => {
                let reason = match error {
                    RoutingError::BudgetExhausted => Some(SearchStop::BudgetExhausted),
                    RoutingError::DeadlineExceeded => Some(SearchStop::DeadlineExceeded),
                    _ => None,
                };
                if let Some(reason) = reason {
                    self.stop_reason.store(reason as u8, Ordering::Release);
                }
                Err(error)
            }
            Ok(None) => {
                self.stop_reason
                    .store(SearchStop::BudgetExhausted as u8, Ordering::Release);
                Err(RoutingError::BudgetExhausted)
            }
            Err(WorkerError::JobStateUnavailable) => {
                self.stop_reason
                    .store(SearchStop::JobStateUnavailable as u8, Ordering::Release);
                Err(RoutingError::Unavailable)
            }
            Err(_) => {
                self.stop_reason
                    .store(SearchStop::ProviderTaskFailed as u8, Ordering::Release);
                Err(RoutingError::Unavailable)
            }
        }
    }
}

fn job_error(error: JobError) -> WorkerError {
    match error {
        JobError::Unavailable => WorkerError::JobStateUnavailable,
        JobError::InvalidInput => WorkerError::InvalidJobInput,
        JobError::LeaseLost | JobError::Terminal | JobError::NotFound => WorkerError::LeaseLost,
        _ => WorkerError::InvalidJobInput,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ClaimedJobInput {
        ClaimedJobInput {
            owner: Uuid::new_v4(),
            request_revision: 1,
            confirmed_longitude: 77.594011,
            confirmed_latitude: 12.9722064,
            requested_distance_metres: 1_300,
            distance_tolerance_metres: 260,
            return_to_start: true,
            activity_profile_json: r#"{"activity":"walking","profile_version":"walking-v1"}"#
                .into(),
            activity_profile_version: "walking-v1".into(),
            coverage_policy_version: "pilot-v1".into(),
            template_id: "heart".into(),
            template_version: 1,
            map_data_version: "map-v1".into(),
            algorithm_version: SUPPORTED_ALGORITHM_VERSION.into(),
            deadline_at: SystemTime::UNIX_EPOCH + Duration::from_secs(100),
            expires_at: SystemTime::UNIX_EPOCH + Duration::from_secs(200),
        }
    }

    #[test]
    fn immutable_input_validation_rejects_unknown_versions_and_malformed_values() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        assert!(prepare(&input(), "map-v1", "pilot-v1", now).is_ok());
        let mut wrong_map = input();
        wrong_map.map_data_version = "map-v2".into();
        assert!(prepare(&wrong_map, "map-v1", "pilot-v1", now).is_err());
        let mut wrong_template = input();
        wrong_template.template_version = 99;
        assert!(prepare(&wrong_template, "map-v1", "pilot-v1", now).is_err());
        let mut malformed_profile = input();
        malformed_profile.activity_profile_json = "{}".into();
        assert!(prepare(&malformed_profile, "map-v1", "pilot-v1", now).is_err());
        let mut unsupported_finish = input();
        unsupported_finish.return_to_start = false;
        assert!(prepare(&unsupported_finish, "map-v1", "pilot-v1", now).is_err());
        let mut wrong_coverage = input();
        wrong_coverage.coverage_policy_version = "unknown-v9".into();
        assert!(prepare(&wrong_coverage, "map-v1", "pilot-v1", now).is_err());
        let mut noncanonical_start = input();
        noncanonical_start.confirmed_longitude = 77.59401101;
        assert!(prepare(&noncanonical_start, "map-v1", "pilot-v1", now).is_err());
    }

    #[test]
    fn swallowed_search_limits_are_restored_to_truthful_terminal_outcomes() {
        assert_eq!(
            forced_completion(SearchStop::BudgetExhausted, false).unwrap(),
            Some(Completion::NoMatch {
                code: JobOutcomeCode::ExhaustedBudget,
            })
        );
        assert_eq!(
            forced_completion(SearchStop::DeadlineExceeded, false).unwrap(),
            Some(Completion::TimedOut)
        );
        assert!(matches!(
            forced_completion(SearchStop::JobStateUnavailable, false),
            Err(WorkerError::JobStateUnavailable)
        ));
    }
}
