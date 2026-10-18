use axum::{
    Router,
    body::{Body, Bytes},
    http::{Method, Request, StatusCode},
    response::Response,
};
use gpsart_api::auth::SessionPolicy;
use gpsart_api::{ConfiguredStartService, app_with_all_services_and_routes_and_limits};
use gpsart_config::{ProductionRoutingConfig, WorkloadLimits};
use gpsart_domain::starts::StartResolver;
use gpsart_persistence::jobs::PostgresJobCoordinator;
use gpsart_persistence::routes::{PostgresRouteSnapshotStore, RouteSnapshotStore};
use gpsart_persistence::{
    MIGRATOR, PostgresGuestSessionStore, PostgresProbe, PostgresResolvedStartStore,
};
use gpsart_worker::production::ProductionWorker;
use http_body_util::BodyExt;
use sqlx::{
    ConnectOptions, Connection, Executor, PgConnection, PgPool, postgres::PgConnectOptions,
};
use std::{env, fs, io::Cursor, path::PathBuf, str::FromStr, sync::Arc, time::Duration};
use tower::ServiceExt;
use uuid::Uuid;

const ORIGIN: &str = "https://app.test";
const MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../ops/routing/production-pilot-v1.json"
);

struct TestDatabase {
    admin: PgConnection,
    pool: PgPool,
    name: String,
    url: String,
}

impl TestDatabase {
    async fn create() -> Self {
        let database_url = env::var("DATABASE_URL").expect(
            "TS-502 is a required PostgreSQL/PostGIS test; DATABASE_URL must be configured",
        );
        let mut admin = PgConnection::connect(&database_url)
            .await
            .expect("DATABASE_URL must identify an available PostgreSQL administrator database");
        let name = format!(
            "gpsart_connected_slice_{}_{}",
            std::process::id(),
            Uuid::new_v4().simple()
        );
        admin
            .execute(format!("CREATE DATABASE {name}").as_str())
            .await
            .expect("isolated TS-502 database creation must succeed");
        let options = PgConnectOptions::from_str(&database_url)
            .expect("DATABASE_URL must be valid")
            .database(&name);
        let url = options.to_url_lossy().to_string();
        let pool = PgPool::connect_with(options)
            .await
            .expect("isolated TS-502 database must accept connections");
        MIGRATOR.run(&pool).await.expect("migrations must succeed");
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
            .expect("isolated TS-502 database cleanup must succeed");
    }
}

#[derive(Clone)]
struct Guest {
    cookie: String,
    csrf: String,
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

async fn response_json(response: Response) -> serde_json::Value {
    serde_json::from_slice(&response_bytes(response).await).expect("JSON response")
}

async fn request(
    app: &Router,
    method: Method,
    uri: impl AsRef<str>,
    guest: Option<&Guest>,
    body: Option<serde_json::Value>,
) -> Response {
    let mut builder = Request::builder().method(method).uri(uri.as_ref());
    if let Some(guest) = guest {
        builder = builder.header("cookie", &guest.cookie);
    }
    if body.is_some() {
        builder = builder
            .header("content-type", "application/json")
            .header("origin", ORIGIN);
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
        .expect("router response")
}

async fn create_guest(app: &Router) -> Guest {
    let response = request(
        app,
        Method::POST,
        "/api/v1/guest-sessions",
        None,
        Some(serde_json::json!({})),
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
                .expect("cookie header")
                .split(';')
                .next()
                .expect("cookie pair")
                .to_owned()
        })
        .collect::<Vec<_>>();
    let csrf = cookies
        .iter()
        .find_map(|cookie| cookie.strip_prefix("gpsart_csrf="))
        .expect("CSRF cookie")
        .to_owned();
    Guest {
        cookie: cookies.join("; "),
        csrf,
    }
}

async fn connected_app(database: &TestDatabase) -> (Router, ProductionWorker) {
    let routing = ProductionRoutingConfig::load(MANIFEST).expect("approved routing manifest");
    let limits = WorkloadLimits::default();
    routing
        .validate_workload(&limits)
        .expect("reviewed workload");
    let starts = ConfiguredStartService::with_store(
        StartResolver::new(
            routing.coverage().clone(),
            routing.network_provider().expect("pinned network provider"),
            Duration::from_secs(300),
        ),
        routing.map_data_version(),
        Arc::new(
            PostgresResolvedStartStore::connect(&database.url)
                .await
                .expect("resolved-start store"),
        ),
    );
    let sessions = Arc::new(
        PostgresGuestSessionStore::connect(&database.url)
            .await
            .expect("guest-session store"),
    );
    let jobs = PostgresJobCoordinator::connect_with_limits(&database.url, limits.clone())
        .await
        .expect("job coordinator");
    let routes = PostgresRouteSnapshotStore::connect(&database.url)
        .await
        .expect("route store");
    let app = app_with_all_services_and_routes_and_limits(
        PostgresProbe::new(database.url.clone(), Duration::from_secs(2)),
        starts,
        sessions,
        SessionPolicy::new(
            ORIGIN,
            true,
            Duration::from_secs(86_400),
            Duration::from_secs(43_200),
        )
        .expect("session policy"),
        jobs,
        routes,
        limits.clone(),
    );
    let (graph, checksum) = graph_paths();
    let worker = ProductionWorker::connect(
        &database.url,
        &graph,
        &checksum,
        &routing.coverage().version,
        limits,
    )
    .await
    .expect("production worker");
    (app, worker)
}

#[tokio::test]
async fn anonymous_browser_api_postgis_worker_and_gpx_share_one_immutable_route() {
    let database = TestDatabase::create().await;
    let (app, worker) = connected_app(&database).await;
    let owner = create_guest(&app).await;

    let catalogue = request(&app, Method::GET, "/api/v1/designs", None, None).await;
    assert_eq!(catalogue.status(), StatusCode::OK);
    let catalogue = response_json(catalogue).await;
    let heart = catalogue["designs"]
        .as_array()
        .expect("catalogue designs")
        .iter()
        .find(|design| design["id"] == "heart")
        .expect("published heart design");
    assert_eq!(heart["version"], 1);

    let profile = serde_json::json!({"activity":"walking","profile_version":"walking-v1"});
    let resolved = request(
        &app,
        Method::POST,
        "/api/v1/starts/resolve",
        Some(&owner),
        Some(serde_json::json!({
            "requested_point":{"longitude":77.5939555,"latitude":12.9724454},
            "profile":profile
        })),
    )
    .await;
    assert_eq!(resolved.status(), StatusCode::OK);
    let resolved = response_json(resolved).await;
    assert_eq!(resolved["snap_distance_metres"], 0);
    let resolution_id = resolved["id"].as_str().expect("resolution id");
    let confirmed = request(
        &app,
        Method::POST,
        format!("/api/v1/starts/{resolution_id}/confirm"),
        Some(&owner),
        Some(serde_json::json!({"profile":profile})),
    )
    .await;
    assert_eq!(confirmed.status(), StatusCode::OK);
    let confirmed = response_json(confirmed).await;
    assert_eq!(
        confirmed["confirmed_start"],
        resolved["candidate_network_point"]
    );

    let accepted = request(
        &app,
        Method::POST,
        "/api/v1/route-jobs",
        Some(&owner),
        Some(serde_json::json!({
            "resolved_start_id":resolution_id,
            "revision":1,
            "idempotency_key":"ts-502-connected-heart-v1",
            "requested_distance_metres":1300,
            "distance_tolerance_metres":260,
            "return_to_start":true,
            "profile":profile,
            "coverage_policy_version":"bengaluru-central-pilot-coverage-v1",
            "template_id":"heart",
            "template_version":"1",
            "map_data_version":"osm-bengaluru-central-pedestrian-pilot-v3:sha256:df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45",
            "algorithm_version":"algorithm-v1"
        })),
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let accepted = response_json(accepted).await;
    let job_id = accepted["job_id"].as_str().expect("job id");
    assert_eq!(accepted["request_revision"], 1);

    let queued = request(
        &app,
        Method::GET,
        format!("/api/v1/route-jobs/{job_id}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(queued.status(), StatusCode::OK);
    let queued = response_json(queued).await;
    assert_eq!(queued["state"], "queued");
    assert_eq!(queued["stage"], "queued");

    worker.take_one().await.expect("real route generation");
    let completed = request(
        &app,
        Method::GET,
        format!("/api/v1/route-jobs/{job_id}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(completed.status(), StatusCode::OK);
    let completed = response_json(completed).await;
    assert_eq!(completed["state"], "succeeded");
    assert_eq!(completed["request_revision"], 1);
    assert!(
        completed["status_version"].as_i64().unwrap_or_default() >= 4,
        "claim, fitting, comparison, and terminal progress must be persisted"
    );
    let candidate_id = completed["result_ids"][0].as_str().expect("candidate id");

    let details_response = request(
        &app,
        Method::GET,
        format!("/api/v1/routes/{candidate_id}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(details_response.status(), StatusCode::OK);
    let details = response_json(details_response).await;
    assert_eq!(details["id"], candidate_id);
    assert_eq!(details["generation_job_id"], job_id);
    assert_eq!(details["request_revision"], 1);
    assert_eq!(
        details["versions"]["routing_data"],
        "osm-bengaluru-central-pedestrian-pilot-v3:sha256:df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45"
    );
    assert_eq!(details["versions"]["template_id"], "heart");
    assert_eq!(
        details["versions"]["candidate"],
        details["candidate_version"]
    );
    assert_eq!(details["distances"]["repeated_included_in_total"], true);

    let owner_id: Uuid =
        sqlx::query_scalar("SELECT id FROM guest_sessions ORDER BY created_at LIMIT 1")
            .fetch_one(&database.pool)
            .await
            .expect("persisted owner");
    let route_id = Uuid::parse_str(candidate_id).expect("candidate UUID");
    let stored = PostgresRouteSnapshotStore::connect(&database.url)
        .await
        .expect("snapshot store")
        .get(owner_id, route_id)
        .await
        .expect("persisted candidate");
    assert_eq!(
        serde_json::to_value(&stored).unwrap()["geometry"],
        details["geometry"]
    );
    assert_eq!(
        serde_json::to_value(&stored).unwrap()["distances"],
        details["distances"]
    );
    assert_eq!(
        serde_json::to_value(&stored).unwrap()["versions"],
        details["versions"]
    );

    let gpx_response = request(
        &app,
        Method::GET,
        format!("/api/v1/routes/{candidate_id}/export.gpx"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(gpx_response.status(), StatusCode::OK);
    assert_eq!(
        gpx_response.headers()["content-type"],
        "application/gpx+xml; charset=utf-8"
    );
    let gpx_bytes = response_bytes(gpx_response).await;
    let gpx_text = std::str::from_utf8(&gpx_bytes).expect("GPX is UTF-8");
    for expected_metadata in [
        candidate_id,
        "candidate version 1",
        "profile walking-v1",
        "routing data osm-bengaluru-central-pedestrian-pilot-v3:sha256:df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45",
        "including connectors and retraced sections",
    ] {
        assert!(
            gpx_text.contains(expected_metadata),
            "GPX must retain its documented immutable identity and traversal disclosure"
        );
    }
    let imported = gpx::read(Cursor::new(gpx_bytes))
        .expect("external GPX 1.1 importer accepts the connected export");
    let gpx_coordinates = imported.tracks[0].segments[0]
        .points
        .iter()
        .map(|point| [point.point().x(), point.point().y()])
        .collect::<Vec<_>>();
    let detail_coordinates: Vec<[f64; 2]> =
        serde_json::from_value(details["geometry"]["coordinates"].clone()).unwrap();
    assert_eq!(gpx_coordinates, detail_coordinates);
    assert_eq!(stored.geometry.coordinates, detail_coordinates);

    if let Ok(path) = env::var("GPSART_CONNECTED_TRANSCRIPT_PATH") {
        fs::write(
            path,
            serde_json::to_vec(&details).expect("serialize live API transcript"),
        )
        .expect("write ephemeral live API transcript for the browser verifier");
    }

    let foreign = create_guest(&app).await;
    for uri in [
        format!("/api/v1/route-jobs/{job_id}"),
        format!("/api/v1/routes/{candidate_id}"),
        format!("/api/v1/routes/{candidate_id}/export.gpx"),
        format!("/api/v1/route-jobs/{}", Uuid::new_v4()),
        format!("/api/v1/routes/{}", Uuid::new_v4()),
    ] {
        assert_eq!(
            request(&app, Method::GET, uri, Some(&foreign), None)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        request(
            &app,
            Method::DELETE,
            format!("/api/v1/route-jobs/{job_id}"),
            Some(&foreign),
            Some(serde_json::json!({})),
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    for uri in [
        "/api/v1/route-jobs/not-a-uuid",
        "/api/v1/routes/not-a-uuid",
        "/api/v1/routes/not-a-uuid/export.gpx",
    ] {
        assert_eq!(
            request(&app, Method::GET, uri, Some(&foreign), None)
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }

    database.drop().await;
}
