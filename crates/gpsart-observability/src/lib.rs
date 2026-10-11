use serde::Serialize;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceBucket {
    UnderOneKm,
    OneToFiveKm,
    FiveToTenKm,
    TenToTwentyKm,
    TwentyToFiftyKm,
    FiftyToOneHundredKm,
    OverOneHundredKm,
}

impl DistanceBucket {
    pub const fn from_metres(metres: i32) -> Self {
        match metres {
            ..=999 => Self::UnderOneKm,
            1_000..=4_999 => Self::OneToFiveKm,
            5_000..=9_999 => Self::FiveToTenKm,
            10_000..=19_999 => Self::TenToTwentyKm,
            20_000..=49_999 => Self::TwentyToFiftyKm,
            50_000..=100_000 => Self::FiftyToOneHundredKm,
            _ => Self::OverOneHundredKm,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnderOneKm => "under_1km",
            Self::OneToFiveKm => "1_to_5km",
            Self::FiveToTenKm => "5_to_10km",
            Self::TenToTwentyKm => "10_to_20km",
            Self::TwentyToFiftyKm => "20_to_50km",
            Self::FiftyToOneHundredKm => "50_to_100km",
            Self::OverOneHundredKm => "over_100km",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignCategory {
    Shape,
    Text,
}

impl DesignCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shape => "shape",
            Self::Text => "text",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RouteDimensions {
    activity: String,
    distance_bucket: DistanceBucket,
    design_category: DesignCategory,
    profile_version: String,
    template_version: String,
    map_data_version: String,
    algorithm_version: String,
}

impl RouteDimensions {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        activity: impl Into<String>,
        distance_bucket: DistanceBucket,
        design_category: DesignCategory,
        profile_version: impl Into<String>,
        template_version: impl Into<String>,
        map_data_version: impl Into<String>,
        algorithm_version: impl Into<String>,
    ) -> Result<Self, InvalidDimension> {
        let value = Self {
            activity: activity.into(),
            distance_bucket,
            design_category,
            profile_version: profile_version.into(),
            template_version: template_version.into(),
            map_data_version: map_data_version.into(),
            algorithm_version: algorithm_version.into(),
        };
        for identifier in [
            &value.activity,
            &value.profile_version,
            &value.map_data_version,
            &value.algorithm_version,
        ] {
            if !is_machine_identifier(identifier) {
                return Err(InvalidDimension);
            }
        }
        if !is_template_version(&value.template_version) {
            return Err(InvalidDimension);
        }
        Ok(value)
    }
}

fn is_machine_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_alphabetic()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn is_template_version(value: &str) -> bool {
    (!value.is_empty() && value.len() <= 10 && value.bytes().all(|byte| byte.is_ascii_digit()))
        || is_machine_identifier(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidDimension;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Correlation {
    pub correlation_id: Uuid,
    pub job_id: Uuid,
    pub attempt: Option<u32>,
    pub provider_call: Option<u32>,
}

impl Correlation {
    pub fn system() -> Self {
        Self::for_job(Uuid::new_v4())
    }

    pub const fn for_job(job_id: Uuid) -> Self {
        Self {
            correlation_id: job_id,
            job_id,
            attempt: None,
            provider_call: None,
        }
    }

    pub const fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = Some(attempt);
        self
    }

    pub const fn with_provider_call(mut self, provider_call: u32) -> Self {
        self.provider_call = Some(provider_call);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricName {
    AcknowledgementLatency,
    QueueDelay,
    SearchLatency,
    ProviderLatency,
    TotalLatency,
    TerminalOutcome,
    Cancellation,
    Timeout,
    ProviderCalls,
    CandidatesReturned,
    QueueBacklog,
    StuckLeases,
    ProviderFailures,
    Errors,
    CleanupFailure,
}

impl MetricName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AcknowledgementLatency => "acknowledgement_latency_ms",
            Self::QueueDelay => "queue_delay_ms",
            Self::SearchLatency => "search_latency_ms",
            Self::ProviderLatency => "provider_latency_ms",
            Self::TotalLatency => "total_latency_ms",
            Self::TerminalOutcome => "terminal_outcome_total",
            Self::Cancellation => "cancellation_total",
            Self::Timeout => "timeout_total",
            Self::ProviderCalls => "provider_calls_total",
            Self::CandidatesReturned => "candidates_returned_total",
            Self::QueueBacklog => "queue_backlog",
            Self::StuckLeases => "stuck_leases",
            Self::ProviderFailures => "provider_failures_total",
            Self::Errors => "errors_total",
            Self::CleanupFailure => "cleanup_failures_total",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricValue {
    DurationMilliseconds(u64),
    Count(u64),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MetricEvent {
    pub metric: MetricName,
    pub correlation: Correlation,
    pub value: MetricValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<RouteDimensions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result_class: Option<String>,
}

impl MetricEvent {
    pub fn duration(
        metric: MetricName,
        correlation: Correlation,
        duration: Duration,
        dimensions: Option<RouteDimensions>,
    ) -> Self {
        Self {
            metric,
            correlation,
            value: MetricValue::DurationMilliseconds(
                duration.as_millis().min(u128::from(u64::MAX)) as u64,
            ),
            dimensions,
            result_class: None,
        }
    }

    pub const fn count(
        metric: MetricName,
        correlation: Correlation,
        count: u64,
        dimensions: Option<RouteDimensions>,
    ) -> Self {
        Self {
            metric,
            correlation,
            value: MetricValue::Count(count),
            dimensions,
            result_class: None,
        }
    }

    pub fn with_result_class(mut self, result_class: &str) -> Self {
        self.result_class = Some(if is_allowed_result_class(result_class) {
            result_class.to_owned()
        } else {
            "redacted_invalid_result_class".to_owned()
        });
        self
    }

    pub fn emit(&self) {
        let duration_ms = match self.value {
            MetricValue::DurationMilliseconds(value) => Some(value),
            MetricValue::Count(_) => None,
        };
        let count = match self.value {
            MetricValue::Count(value) => Some(value),
            MetricValue::DurationMilliseconds(_) => None,
        };
        let dimensions = self.dimensions.as_ref();
        tracing::info!(
            event = "operational_metric",
            metric = self.metric.as_str(),
            correlation_id = %self.correlation.correlation_id,
            job_id = %self.correlation.job_id,
            attempt = self.correlation.attempt,
            provider_call = self.correlation.provider_call,
            duration_ms,
            count,
            activity = dimensions.map(|value| value.activity.as_str()),
            distance_bucket = dimensions.map(|value| value.distance_bucket.as_str()),
            design_category = dimensions.map(|value| value.design_category.as_str()),
            profile_version = dimensions.map(|value| value.profile_version.as_str()),
            template_version = dimensions.map(|value| value.template_version.as_str()),
            map_data_version = dimensions.map(|value| value.map_data_version.as_str()),
            algorithm_version = dimensions.map(|value| value.algorithm_version.as_str()),
            result_class = self.result_class.as_deref(),
            "operational metric"
        );
    }
}

fn is_allowed_result_class(value: &str) -> bool {
    matches!(
        value,
        "ok" | "failed"
            | "match"
            | "no_match"
            | "cancelled"
            | "timed_out"
            | "invalid_request"
            | "provider_error"
            | "provider_task_failed"
            | "provider_unavailable"
            | "job_state_unavailable"
            | "storage_unavailable"
            | "succeeded"
            | "unsupported_coverage"
            | "no_routable_access"
            | "disconnected_network"
            | "insufficient_visual_match"
            | "outside_distance"
            | "exhausted_budget"
            | "attempts_exhausted"
            | "legacy_unknown"
    )
}
