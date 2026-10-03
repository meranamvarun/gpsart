use async_trait::async_trait;
use axum::{body::Body, http::Request};
use gpsart_api::{
    CoveragePayload, Readiness, RouteJobService, StartService, app_with_all_services,
    auth::{InMemorySessionStore, SessionPolicy},
};
use gpsart_domain::{
    RouteRequestProfile,
    starts::{Point, ResolutionId, ResolveError, StartResolution},
};
use gpsart_persistence::jobs::{
    JobError, JobOutcomeCode, JobOutcomeSnapshot, JobStage, JobState, JobStatusSnapshot,
    RouteJobSubmission,
};
use http_body_util::BodyExt;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Clone)]
struct Healthy;

#[async_trait]
impl Readiness for Healthy {
    async fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

struct NoStarts;

#[async_trait]
impl StartService for NoStarts {
    fn coverage(&self) -> CoveragePayload {
        CoveragePayload {
            version: "v".into(),
            geojson: serde_json::json!({}),
            local_route_feasibility: "not_evaluated",
        }
    }
    async fn resolve(
        &self,
        _: &str,
        _: Point,
        _: RouteRequestProfile,
    ) -> Result<StartResolution, ResolveError> {
        Err(ResolveError::ProviderFailure)
    }
    async fn confirm(
        &self,
        _: &str,
        _: ResolutionId,
        _: RouteRequestProfile,
    ) -> Result<Point, ResolveError> {
        Err(ResolveError::ProviderFailure)
    }
}

type StoredStatus = (Option<Uuid>, JobStatusSnapshot);

#[derive(Clone, Default)]
struct Jobs(Arc<Mutex<HashMap<Uuid, StoredStatus>>>);

#[async_trait]
impl RouteJobService for Jobs {
    async fn submit(&self, _: Uuid, _: RouteJobSubmission) -> Result<Uuid, JobError> {
        Err(JobError::Unavailable)
    }

    async fn cancel(&self, _: Uuid, _: Uuid) -> Result<JobState, JobError> {
        Err(JobError::Unavailable)
    }

    async fn status(&self, owner: Uuid, job_id: Uuid) -> Result<JobStatusSnapshot, JobError> {
        let mut jobs = self.0.lock().unwrap();
        let (bound_owner, snapshot) = jobs.get_mut(&job_id).ok_or(JobError::NotFound)?;
        match bound_owner {
            Some(bound) if *bound != owner => Err(JobError::NotFound),
            Some(_) => Ok(snapshot.clone()),
            slot @ None => {
                *slot = Some(owner);
                Ok(snapshot.clone())
            }
        }
    }
}

fn policy() -> SessionPolicy {
    SessionPolicy::new(
        "https://app.test",
        true,
        Duration::from_secs(86_400),
        Duration::from_secs(43_200),
    )
    .unwrap()
}

async fn auth(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/v1/guest-sessions")
                .header("origin", "https://app.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|value| value.to_str().unwrap().split(';').next().unwrap())
        .collect::<Vec<_>>()
        .join("; ")
}

fn snapshot(job_id: Uuid, state: JobState, outcome: Option<JobOutcomeCode>) -> JobStatusSnapshot {
    let epoch = SystemTime::UNIX_EPOCH;
    JobStatusSnapshot {
        job_id,
        request_revision: 3,
        state,
        stage: if state == JobState::Queued {
            JobStage::Queued
        } else {
            JobStage::FittingDesign
        },
        status_version: 7,
        queued_at: epoch + Duration::from_secs(1_800_000_000),
        started_at: (state != JobState::Queued)
            .then_some(epoch + Duration::from_secs(1_800_000_001)),
        completed_at: state
            .is_terminal()
            .then_some(epoch + Duration::from_secs(1_800_000_055)),
        deadline_at: epoch + Duration::from_secs(1_800_000_060),
        expires_at: epoch + Duration::from_secs(1_800_086_400),
        outcome: outcome.map(|code| JobOutcomeSnapshot {
            code,
            budget_exhausted: matches!(
                code,
                JobOutcomeCode::ExhaustedBudget | JobOutcomeCode::TimedOut
            ),
        }),
        result_ids: if state == JobState::Succeeded {
            vec![Uuid::new_v4()]
        } else {
            Vec::new()
        },
    }
}

async fn get(
    app: &axum::Router,
    cookie: &str,
    job_id: Uuid,
    etag: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::get(format!("/api/v1/route-jobs/{job_id}")).header("cookie", cookie);
    if let Some(etag) = etag {
        request = request.header("if-none-match", etag);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn owned_status_supports_monotonic_etags_without_invented_progress() {
    let jobs = Jobs::default();
    let job_id = Uuid::new_v4();
    jobs.0
        .lock()
        .unwrap()
        .insert(job_id, (None, snapshot(job_id, JobState::Running, None)));
    let app = app_with_all_services(
        Healthy,
        NoStarts,
        Arc::new(InMemorySessionStore::default()),
        policy(),
        jobs,
    );
    let owner_cookie = auth(&app).await;
    let response = get(&app, &owner_cookie, job_id, None).await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["etag"], "\"7\"");
    assert_eq!(response.headers()["cache-control"], "private, no-cache");
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["state"], "running");
    assert_eq!(body["stage"], "fitting_design");
    assert_eq!(body["status_version"], 7);
    assert_eq!(body["request_revision"], 3);
    assert_eq!(body["poll_after_seconds"], 1);
    assert!(body.get("percentage").is_none());

    let unchanged = get(&app, &owner_cookie, job_id, Some("\"7\"")).await;
    assert_eq!(unchanged.status(), 304);
    assert_eq!(unchanged.headers()["etag"], "\"7\"");
    assert!(
        unchanged
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );
    assert_eq!(
        get(&app, &owner_cookie, job_id, Some("*")).await.status(),
        304
    );

    let foreign_cookie = auth(&app).await;
    assert_eq!(get(&app, &foreign_cookie, job_id, None).await.status(), 404);
    assert_eq!(
        get(&app, &owner_cookie, Uuid::new_v4(), None)
            .await
            .status(),
        404
    );
}

#[tokio::test]
async fn every_terminal_class_serializes_with_consistent_state_and_retry_guidance() {
    let jobs = Jobs::default();
    let cases = [
        (JobState::Succeeded, JobOutcomeCode::Succeeded),
        (JobState::NoMatch, JobOutcomeCode::UnsupportedCoverage),
        (JobState::NoMatch, JobOutcomeCode::NoRoutableAccess),
        (JobState::NoMatch, JobOutcomeCode::DisconnectedNetwork),
        (JobState::NoMatch, JobOutcomeCode::InsufficientVisualMatch),
        (JobState::NoMatch, JobOutcomeCode::OutsideDistance),
        (JobState::NoMatch, JobOutcomeCode::ExhaustedBudget),
        (JobState::Failed, JobOutcomeCode::ProviderUnavailable),
        (JobState::Failed, JobOutcomeCode::AttemptsExhausted),
        (JobState::Cancelled, JobOutcomeCode::Cancelled),
        (JobState::TimedOut, JobOutcomeCode::TimedOut),
    ];
    for (state, code) in cases {
        let id = Uuid::new_v4();
        jobs.0
            .lock()
            .unwrap()
            .insert(id, (None, snapshot(id, state, Some(code))));
    }
    let ids = jobs.0.lock().unwrap().keys().copied().collect::<Vec<_>>();
    let app = app_with_all_services(
        Healthy,
        NoStarts,
        Arc::new(InMemorySessionStore::default()),
        policy(),
        jobs,
    );
    let cookie = auth(&app).await;
    for id in ids {
        let response = get(&app, &cookie, id, None).await;
        assert_eq!(response.status(), 200);
        let body: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert!(body["completed_at_epoch_seconds"].is_number());
        assert!(body["outcome"]["code"].is_string());
        assert!(body["poll_after_seconds"].is_null());
        if body["outcome"]["code"] == "provider_unavailable" {
            assert_eq!(body["outcome"]["retryable"], true);
            assert_eq!(body["outcome"]["retry_after_seconds"], 30);
        }
    }
}
