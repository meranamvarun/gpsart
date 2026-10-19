use axum::{
    Router,
    body::{Body, Bytes},
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use gpsart_api::auth::SessionPolicy;
use gpsart_api::{ConfiguredStartService, app_with_all_services_and_routes_and_limits};
use gpsart_config::{ProductionRoutingConfig, WorkloadLimits};
use gpsart_domain::starts::StartResolver;
use gpsart_persistence::jobs::PostgresJobCoordinator;
use gpsart_persistence::routes::PostgresRouteSnapshotStore;
use gpsart_persistence::{
    GuestSessionStore, MIGRATOR, PostgresGuestSessionStore, PostgresProbe,
    PostgresResolvedStartStore,
};
use gpsart_worker::production::{ProductionWorker, WorkerJobProfile};
use http_body_util::BodyExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{
    ConnectOptions, Connection, Executor, PgConnection, PgPool, Row, postgres::PgConnectOptions,
};
use std::{
    env,
    path::PathBuf,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};
use tower::ServiceExt;
use uuid::Uuid;

const ORIGIN: &str = "https://performance.test";
const MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ops/routing/production-pilot-v1.json"
);
const WORKLOAD: &str = include_str!("../../../ops/performance/ts506-workload.json");

#[derive(Clone, Deserialize)]
struct WorkloadCase {
    id: String,
    category: String,
    template_id: String,
    activity: String,
    requested_distance_metres: i32,
}

#[derive(Deserialize)]
struct Workload {
    concurrent_jobs: usize,
    production_worker_replicas: usize,
    map_data_version: String,
    algorithm_version: String,
    cases: Vec<WorkloadCase>,
}

struct TestDatabase {
    admin: PgConnection,
    pool: PgPool,
    name: String,
    url: String,
}

impl TestDatabase {
    async fn create() -> Self {
        let database_url = env::var("DATABASE_URL").expect(
            "TS-506 is a required PostgreSQL/PostGIS performance test; DATABASE_URL must be configured",
        );
        let mut admin = PgConnection::connect(&database_url)
            .await
            .expect("administrator database");
        let name = format!(
            "gpsart_production_load_{}_{}",
            std::process::id(),
            Uuid::new_v4().simple()
        );
        admin
            .execute(format!("CREATE DATABASE {name}").as_str())
            .await
            .expect("create isolated database");
        let options = PgConnectOptions::from_str(&database_url)
            .expect("DATABASE_URL")
            .database(&name);
        let url = options.to_url_lossy().to_string();
        let pool = PgPool::connect_with(options)
            .await
            .expect("isolated database");
        MIGRATOR.run(&pool).await.expect("migrations");
        Self {
            admin,
            pool,
            name,
            url,
        }
    }

    async fn drop(mut self) {
        self.pool.close().await;
        self.admin
            .execute(format!("DROP DATABASE {} WITH (FORCE)", self.name).as_str())
            .await
            .expect("drop isolated database");
    }
}

#[derive(Clone)]
struct Guest {
    cookie: String,
    csrf: String,
}

struct Submitted {
    guest: Guest,
    job_id: Uuid,
    acknowledgement: Duration,
}

fn graph_paths() -> (PathBuf, PathBuf) {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../gpsart-routing/data");
    (
        data.join("bengaluru-central-osm-v3.json"),
        data.join("bengaluru-central-osm-v3.sha256"),
    )
}

async fn response_bytes(response: Response) -> Bytes {
    response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes()
}

async fn response_json(response: Response) -> Value {
    serde_json::from_slice(&response_bytes(response).await).expect("JSON response")
}

async fn request(
    app: &Router,
    method: Method,
    uri: impl AsRef<str>,
    guest: Option<&Guest>,
    body: Option<Value>,
) -> Response {
    let mut builder = Request::builder().method(method).uri(uri.as_ref());
    if let Some(guest) = guest {
        builder = builder.header("cookie", &guest.cookie);
    }
    if body.is_some() || matches!(builder.method_ref(), Some(&Method::DELETE)) {
        builder = builder.header("origin", ORIGIN);
        if body.is_some() {
            builder = builder.header("content-type", "application/json");
        }
        if let Some(guest) = guest {
            builder = builder.header("x-csrf-token", &guest.csrf);
        }
    }
    app.clone()
        .oneshot(
            builder
                .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                .expect("request"),
        )
        .await
        .expect("router")
}

async fn create_guest(app: &Router) -> Guest {
    let response = request(
        app,
        Method::POST,
        "/api/v1/guest-sessions",
        None,
        Some(json!({})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookies = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|value| {
            value
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let csrf = cookies
        .iter()
        .find_map(|cookie| cookie.strip_prefix("gpsart_csrf="))
        .unwrap()
        .to_owned();
    Guest {
        cookie: cookies.join("; "),
        csrf,
    }
}

async fn resolve_start(app: &Router, guest: &Guest, case: &WorkloadCase) -> String {
    let (longitude, latitude) = if case.category == "text" {
        (77.6016974, 12.9755777)
    } else {
        (77.5939555, 12.9724454)
    };
    let profile =
        json!({"activity":case.activity,"profile_version":format!("{}-v1", case.activity)});
    let resolved = request(
        app,
        Method::POST,
        "/api/v1/starts/resolve",
        Some(guest),
        Some(json!({
            "requested_point":{"longitude":longitude,"latitude":latitude}, "profile":profile
        })),
    )
    .await;
    assert_eq!(resolved.status(), StatusCode::OK);
    let id = response_json(resolved).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let confirmed = request(
        app,
        Method::POST,
        format!("/api/v1/starts/{id}/confirm"),
        Some(guest),
        Some(json!({"profile":profile})),
    )
    .await;
    assert_eq!(confirmed.status(), StatusCode::OK);
    id
}

async fn submit(app: &Router, workload: &Workload, case: &WorkloadCase) -> Submitted {
    let guest = create_guest(app).await;
    let resolved_start_id = resolve_start(app, &guest, case).await;
    let profile =
        json!({"activity":case.activity,"profile_version":format!("{}-v1", case.activity)});
    let started = Instant::now();
    let response = request(app, Method::POST, "/api/v1/route-jobs", Some(&guest), Some(json!({
        "resolved_start_id":resolved_start_id, "revision":1, "idempotency_key":format!("ts-506-{}", case.id),
        "requested_distance_metres":case.requested_distance_metres, "distance_tolerance_metres":case.requested_distance_metres / 5,
        "return_to_start":true, "profile":profile, "coverage_policy_version":"bengaluru-central-pilot-coverage-v1",
        "template_id":case.template_id, "template_version":"1", "map_data_version":workload.map_data_version,
        "algorithm_version":workload.algorithm_version
    }))).await;
    let acknowledgement = started.elapsed();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let job_id = response_json(response).await["job_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    Submitted {
        guest,
        job_id,
        acknowledgement,
    }
}

async fn connected_app(database: &TestDatabase) -> (Router, Vec<ProductionWorker>) {
    let routing = ProductionRoutingConfig::load(MANIFEST).expect("approved routing manifest");
    let limits = WorkloadLimits::default();
    routing
        .validate_workload(&limits)
        .expect("reviewed workload");
    let starts = ConfiguredStartService::with_store(
        StartResolver::new(
            routing.coverage().clone(),
            routing.network_provider().unwrap(),
            Duration::from_secs(300),
        ),
        routing.map_data_version(),
        Arc::new(
            PostgresResolvedStartStore::connect(&database.url)
                .await
                .unwrap(),
        ),
    );
    let app = app_with_all_services_and_routes_and_limits(
        PostgresProbe::new(database.url.clone(), Duration::from_secs(2)),
        starts,
        Arc::new(
            PostgresGuestSessionStore::connect(&database.url)
                .await
                .unwrap(),
        ),
        SessionPolicy::new(
            ORIGIN,
            true,
            Duration::from_secs(86_400),
            Duration::from_secs(43_200),
        )
        .unwrap(),
        PostgresJobCoordinator::connect_with_limits(&database.url, limits.clone())
            .await
            .unwrap(),
        PostgresRouteSnapshotStore::connect(&database.url)
            .await
            .unwrap(),
        limits.clone(),
    );
    let (graph, checksum) = graph_paths();
    let mut workers = Vec::new();
    for _ in 0..2 {
        workers.push(
            ProductionWorker::connect(
                &database.url,
                &graph,
                &checksum,
                &routing.coverage().version,
                limits.clone(),
            )
            .await
            .unwrap(),
        );
    }
    (app, workers)
}

fn percentile(values: &[f64], rank: usize) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    values[(values.len() * rank).div_ceil(100).saturating_sub(1)]
}

fn distribution(values: &[f64]) -> Value {
    json!({"p50":percentile(values, 50),"p95":percentile(values, 95),"p99":percentile(values, 99)})
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn update_peak(peak: &AtomicUsize, value: usize) {
    let mut current = peak.load(Ordering::Relaxed);
    while value > current {
        match peak.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "recorded production-equivalent load; run with npm run test:performance"]
async fn twenty_connected_jobs_meet_the_immutable_production_gates() {
    let workload: Workload = serde_json::from_str(WORKLOAD).unwrap();
    assert_eq!(workload.concurrent_jobs, 20);
    assert_eq!(workload.cases.len(), 20);
    assert_eq!(workload.production_worker_replicas, 2);
    let database = TestDatabase::create().await;
    let (app, workers) = connected_app(&database).await;

    let overload_case = workload.cases[0].clone();
    let overload_guest = create_guest(&app).await;
    let overload_start = resolve_start(&app, &overload_guest, &overload_case).await;
    let mut submitted = Vec::new();
    for case in &workload.cases {
        submitted.push(submit(&app, &workload, case).await);
    }
    let peak_queue_depth: i64 =
        sqlx::query_scalar("SELECT count(*) FROM generation_jobs WHERE state='queued'")
            .fetch_one(&database.pool)
            .await
            .unwrap();
    assert_eq!(peak_queue_depth, 20);

    let overload_started = Instant::now();
    let overload = request(&app, Method::POST, "/api/v1/route-jobs", Some(&overload_guest), Some(json!({
        "resolved_start_id":overload_start, "revision":1, "idempotency_key":"ts-506-overload",
        "requested_distance_metres":1300, "distance_tolerance_metres":260, "return_to_start":true,
        "profile":{"activity":"walking","profile_version":"walking-v1"},
        "coverage_policy_version":"bengaluru-central-pilot-coverage-v1", "template_id":"heart", "template_version":"1",
        "map_data_version":workload.map_data_version, "algorithm_version":workload.algorithm_version
    }))).await;
    let overload_latency = overload_started.elapsed();
    assert_eq!(overload.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = overload
        .headers()
        .get(header::RETRY_AFTER)
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let overload_body = response_json(overload).await;

    let cancelled = &submitted[0];
    let cancellation = request(
        &app,
        Method::DELETE,
        format!("/api/v1/route-jobs/{}", cancelled.job_id),
        Some(&cancelled.guest),
        None,
    )
    .await;
    assert_eq!(cancellation.status(), StatusCode::OK);
    assert_eq!(response_json(cancellation).await["state"], "cancelled");

    let cleanup_store = PostgresGuestSessionStore::connect(&database.url)
        .await
        .unwrap();
    let cleanup_started = Instant::now();
    let cleanup = cleanup_store
        .cleanup_expired_sessions(SystemTime::now())
        .await
        .unwrap();
    let cleanup_latency = cleanup_started.elapsed();
    assert!(!cleanup.retry_required);

    let active = Arc::new(AtomicUsize::new(0));
    let peak_active = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();
    for worker in workers {
        let active = active.clone();
        let peak_active = peak_active.clone();
        tasks.push(tokio::spawn(async move {
            let mut profiles = Vec::new();
            for _ in 0..20 {
                let now_active = active.fetch_add(1, Ordering::Relaxed) + 1;
                update_peak(&peak_active, now_active);
                let result = worker.take_one_profiled().await;
                active.fetch_sub(1, Ordering::Relaxed);
                if let Some(profile) = result.unwrap() {
                    profiles.push(profile);
                }
            }
            profiles
        }));
    }
    let mut profiles: Vec<WorkerJobProfile> = Vec::new();
    for task in tasks {
        profiles.extend(task.await.unwrap());
    }
    assert_eq!(
        profiles.len(),
        19,
        "one of the twenty accepted jobs was cancelled before claim"
    );

    let rows = sqlx::query("SELECT state::text AS state, COALESCE(EXTRACT(EPOCH FROM (started_at-queued_at))*1000,0)::float8 AS queue_ms, (EXTRACT(EPOCH FROM (completed_at-queued_at))*1000)::float8 AS total_ms FROM generation_jobs ORDER BY queued_at")
        .fetch_all(&database.pool).await.unwrap();
    assert_eq!(rows.len(), 20);
    let queue = rows
        .iter()
        .map(|row| row.get::<f64, _>("queue_ms"))
        .collect::<Vec<_>>();
    let total = rows
        .iter()
        .map(|row| row.get::<f64, _>("total_ms"))
        .collect::<Vec<_>>();
    let mut outcomes = std::collections::BTreeMap::<String, u64>::new();
    for row in &rows {
        *outcomes.entry(row.get::<String, _>("state")).or_default() += 1;
    }
    assert!(outcomes.keys().all(|state| matches!(
        state.as_str(),
        "succeeded" | "no_match" | "failed" | "cancelled" | "timed_out"
    )));

    let acknowledgements = submitted
        .iter()
        .map(|value| millis(value.acknowledgement))
        .collect::<Vec<_>>();
    let graph_loading = profiles
        .iter()
        .map(|value| millis(value.graph_loading))
        .collect::<Vec<_>>();
    let search = profiles
        .iter()
        .map(|value| millis(value.candidate_search))
        .collect::<Vec<_>>();
    let provider = profiles
        .iter()
        .map(|value| millis(value.provider))
        .collect::<Vec<_>>();
    let validation = profiles
        .iter()
        .map(|value| millis(value.validation))
        .collect::<Vec<_>>();
    let persistence = profiles
        .iter()
        .map(|value| millis(value.snapshot_persistence))
        .collect::<Vec<_>>();
    let acknowledgement_p95 = percentile(&acknowledgements, 95);
    let termination_p95 = percentile(&total, 95);
    let maximum = total.iter().copied().fold(0.0, f64::max);
    assert!(acknowledgement_p95 <= 1_000.0);
    assert!(termination_p95 <= 30_000.0);
    assert!(maximum <= 60_000.0);
    assert!(
        profiles.iter().any(|profile| profile.lease_renewals > 0),
        "a long-running production search must prove lease renewal under load"
    );

    let report = json!({
        "latency_milliseconds":{
            "acknowledgement":distribution(&acknowledgements), "queue":distribution(&queue),
            "graph_loading":distribution(&graph_loading), "candidate_search":distribution(&search),
            "provider":distribution(&provider), "validation":distribution(&validation),
            "snapshot_persistence":distribution(&persistence),
            "end_to_end":{"p50":percentile(&total,50),"p95":termination_p95,"p99":percentile(&total,99),"maximum":maximum}
        },
        "terminal_outcomes":outcomes,
        "overload":{"http_status":429,"retry_after_seconds":retry_after,"body_code":overload_body["code"],"latency_ms":millis(overload_latency)},
        "fairness":{"cancellation_completed_while_saturated":true,"cleanup_completed_while_saturated":true,"cleanup_latency_ms":millis(cleanup_latency),"lease_renewal_stayed_live":true},
        "resource_saturation":{"worker_slots":2,"peak_active_worker_slots":peak_active.load(Ordering::Relaxed),"peak_queue_depth":peak_queue_depth}
    });
    println!("TS506_REPORT={report}");
    database.drop().await;
}
