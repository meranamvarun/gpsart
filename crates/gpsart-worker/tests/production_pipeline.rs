use gpsart_config::WorkloadLimits;
use gpsart_persistence::{
    MIGRATOR,
    jobs::{Completion, JobError, JobState, PostgresJobCoordinator},
};
use gpsart_worker::{WorkerError, production::ProductionWorker};
use sqlx::{
    ConnectOptions, Connection, Executor, PgConnection, PgPool, postgres::PgConnectOptions,
};
use std::{env, path::PathBuf, str::FromStr, time::Duration};
use uuid::Uuid;

async fn isolated_database() -> Option<(PgConnection, PgPool, String, String)> {
    let database_url = match env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!(
                "DATABASE_URL is not set; skipping production worker PostGIS integration test"
            );
            return None;
        }
    };
    let mut admin = PgConnection::connect(&database_url)
        .await
        .expect("DATABASE_URL must identify an available PostgreSQL administrator database");
    let database_name = format!(
        "gpsart_worker_pipeline_{}_{}",
        std::process::id(),
        Uuid::new_v4().simple()
    );
    admin
        .execute(format!("CREATE DATABASE {database_name}").as_str())
        .await
        .expect("test database creation must succeed");
    let options = PgConnectOptions::from_str(&database_url)
        .unwrap()
        .database(&database_name);
    let worker_url = options.to_url_lossy().to_string();
    let pool = PgPool::connect_with(options).await.unwrap();
    Some((admin, pool, database_name, worker_url))
}

async fn drop_database(mut admin: PgConnection, pool: PgPool, database_name: &str) {
    pool.close().await;
    admin
        .execute(format!("DROP DATABASE {database_name} WITH (FORCE)").as_str())
        .await
        .unwrap();
}

fn graph_paths() -> (PathBuf, PathBuf) {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../gpsart-routing/data");
    (
        data.join("bengaluru-central-osm-v3.json"),
        data.join("bengaluru-central-osm-v3.sha256"),
    )
}

struct JobFixture<'a> {
    index: usize,
    template_id: &'a str,
    longitude: f64,
    latitude: f64,
    target: i32,
    profile_json: &'a str,
    algorithm_version: &'a str,
    deadline: &'a str,
}

async fn seed_job(pool: &PgPool, owner: Uuid, fixture: JobFixture<'_>) -> Uuid {
    let map_version = "osm-bengaluru-central-pedestrian-pilot-v3:sha256:df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45";
    let start_id = Uuid::new_v4();
    let request_id = Uuid::new_v4();
    let job_id = Uuid::new_v4();
    sqlx::query("INSERT INTO resolved_starts (id,guest_session_id,requested_pin,confirmed_start,profile_version,coverage_policy_version,map_data_version,expires_at) VALUES ($1,$2,ST_SetSRID(ST_MakePoint($3,$4),4326),ST_SetSRID(ST_MakePoint($3,$4),4326),'walking-v1','pilot-v1',$5,now()+interval '1 day')")
        .bind(start_id).bind(owner).bind(fixture.longitude).bind(fixture.latitude).bind(map_version)
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO planning_requests (id,guest_session_id,resolved_start_id,revision,idempotency_key,input_hash,requested_pin,requested_distance_metres,distance_tolerance_metres,activity_profile,activity_profile_version,coverage_policy_version,template_id,template_version,map_data_version,algorithm_version,return_to_start,expires_at) VALUES ($1,$2,$3,$4,$5,decode(repeat('32',32),'hex'),ST_SetSRID(ST_MakePoint($6,$7),4326),$8,$9,$10::jsonb,'walking-v1','pilot-v1',$11,'1',$12,$13,true,now()+interval '1 day')")
        .bind(request_id).bind(owner).bind(start_id).bind((fixture.index + 1) as i32)
        .bind(format!("pipeline-{}", fixture.index)).bind(fixture.longitude).bind(fixture.latitude).bind(fixture.target)
        .bind(fixture.target / 5).bind(fixture.profile_json).bind(fixture.template_id).bind(map_version)
        .bind(fixture.algorithm_version).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO generation_jobs (id,guest_session_id,planning_request_id,deadline_at,max_attempts) VALUES ($1,$2,$3,now()+$4::interval,3)")
        .bind(job_id).bind(owner).bind(request_id).bind(fixture.deadline)
        .execute(pool).await.unwrap();
    job_id
}

async fn connect_worker(database_url: &str) -> ProductionWorker {
    let (graph, checksum) = graph_paths();
    ProductionWorker::connect(
        database_url,
        &graph,
        &checksum,
        "pilot-v1",
        WorkloadLimits::default(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn production_worker_executes_real_heart_and_text_jobs_without_injected_candidates() {
    let Some((admin, pool, database_name, database_url)) = isolated_database().await else {
        return;
    };
    MIGRATOR.run(&pool).await.unwrap();
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO guest_sessions (id,token_hash,expires_at,cleanup_after) VALUES ($1,decode(repeat('31',32),'hex'),now()+interval '1 day',now()+interval '2 days')")
        .bind(owner).execute(&pool).await.unwrap();
    for (index, (template_id, longitude, latitude, target)) in [
        ("heart", 77.5939555_f64, 12.9724454_f64, 1_300_i32),
        ("HI", 77.6016974_f64, 12.9755777_f64, 1_000_i32),
    ]
    .into_iter()
    .enumerate()
    {
        seed_job(
            &pool,
            owner,
            JobFixture {
                index,
                template_id,
                longitude,
                latitude,
                target,
                profile_json: r#"{"activity":"walking","profile_version":"walking-v1"}"#,
                algorithm_version: "algorithm-v1",
                deadline: "1 minute",
            },
        )
        .await;
    }
    let coordinator = PostgresJobCoordinator::connect(&database_url)
        .await
        .unwrap();
    let crashed_lease = coordinator
        .claim(
            Uuid::new_v4(),
            std::time::SystemTime::now(),
            Duration::from_secs(15),
        )
        .await
        .unwrap()
        .unwrap();
    sqlx::query(
        "UPDATE generation_jobs SET lease_expires_at=now()-interval '1 second' WHERE id=$1",
    )
    .bind(crashed_lease.job_id)
    .execute(&pool)
    .await
    .unwrap();
    let worker = connect_worker(&database_url).await;
    worker.take_one().await.unwrap();
    worker.take_one().await.unwrap();
    let job_ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM generation_jobs ORDER BY queued_at,id")
            .fetch_all(&pool)
            .await
            .unwrap();
    for job_id in job_ids {
        let status = coordinator.status(owner, job_id).await.unwrap();
        assert_eq!(status.state, JobState::Succeeded);
        assert_eq!(status.result_ids.len(), 1);
    }
    let recovered_attempts: i32 =
        sqlx::query_scalar("SELECT attempt_count FROM generation_jobs WHERE id=$1")
            .bind(crashed_lease.job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        recovered_attempts, 2,
        "a new worker recovered the expired lease"
    );
    assert_eq!(
        coordinator
            .complete(
                &crashed_lease,
                &Completion::NoMatch {
                    code: gpsart_persistence::jobs::JobOutcomeCode::InsufficientVisualMatch,
                },
                std::time::SystemTime::now(),
            )
            .await,
        Err(JobError::Terminal),
        "late duplicate delivery from the crashed attempt is fenced",
    );
    let snapshots: i64 = sqlx::query_scalar("SELECT count(*) FROM route_candidate_snapshots")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(snapshots, 2);
    let restarted_worker = connect_worker(&database_url).await;
    restarted_worker.take_one().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM route_candidate_snapshots")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2,
        "duplicate delivery after a worker restart cannot republish candidates",
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn malformed_and_unknown_inputs_fail_closed_in_postgres() {
    let Some((admin, pool, database_name, database_url)) = isolated_database().await else {
        return;
    };
    MIGRATOR.run(&pool).await.unwrap();
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO guest_sessions (id,token_hash,expires_at,cleanup_after) VALUES ($1,decode(repeat('41',32),'hex'),now()+interval '1 day',now()+interval '2 days')")
        .bind(owner).execute(&pool).await.unwrap();
    for (index, profile, algorithm) in [
        (0, r#"{"activity":"walking"}"#, "algorithm-v1"),
        (
            1,
            r#"{"activity":"walking","profile_version":"walking-v1"}"#,
            "unknown-v9",
        ),
    ] {
        seed_job(
            &pool,
            owner,
            JobFixture {
                index,
                template_id: "heart",
                longitude: 77.5939555,
                latitude: 12.9724454,
                target: 1_300,
                profile_json: profile,
                algorithm_version: algorithm,
                deadline: "1 minute",
            },
        )
        .await;
    }
    let worker = connect_worker(&database_url).await;
    worker.take_one().await.unwrap();
    worker.take_one().await.unwrap();
    let states: Vec<String> =
        sqlx::query_scalar("SELECT state::text FROM generation_jobs ORDER BY queued_at,id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(states, vec!["failed", "failed"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM route_candidate_snapshots")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn no_match_timeout_cancellation_and_persistence_failure_stay_truthful() {
    let Some((admin, pool, database_name, database_url)) = isolated_database().await else {
        return;
    };
    MIGRATOR.run(&pool).await.unwrap();
    let owner = Uuid::new_v4();
    sqlx::query("INSERT INTO guest_sessions (id,token_hash,expires_at,cleanup_after) VALUES ($1,decode(repeat('51',32),'hex'),now()+interval '1 day',now()+interval '2 days')")
        .bind(owner).execute(&pool).await.unwrap();
    let no_match = seed_job(
        &pool,
        owner,
        JobFixture {
            index: 0,
            template_id: "heart",
            longitude: 0.0,
            latitude: 0.0,
            target: 1_300,
            profile_json: r#"{"activity":"walking","profile_version":"walking-v1"}"#,
            algorithm_version: "algorithm-v1",
            deadline: "1 minute",
        },
    )
    .await;
    let timed_out = seed_job(
        &pool,
        owner,
        JobFixture {
            index: 1,
            template_id: "heart",
            longitude: 77.5939555,
            latitude: 12.9724454,
            target: 1_300,
            profile_json: r#"{"activity":"walking","profile_version":"walking-v1"}"#,
            algorithm_version: "algorithm-v1",
            deadline: "1 millisecond",
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    let worker = connect_worker(&database_url).await;
    worker.take_one().await.unwrap();
    worker.take_one().await.unwrap();
    let coordinator = PostgresJobCoordinator::connect(&database_url)
        .await
        .unwrap();
    assert_eq!(
        coordinator.status(owner, no_match).await.unwrap().state,
        JobState::NoMatch
    );
    assert_eq!(
        coordinator.status(owner, timed_out).await.unwrap().state,
        JobState::TimedOut
    );

    let cancelled = seed_job(
        &pool,
        owner,
        JobFixture {
            index: 2,
            template_id: "heart",
            longitude: 77.5939555,
            latitude: 12.9724454,
            target: 1_300,
            profile_json: r#"{"activity":"walking","profile_version":"walking-v1"}"#,
            algorithm_version: "algorithm-v1",
            deadline: "1 minute",
        },
    )
    .await;
    // Hold the worker between its durable claim and input loading. This makes
    // cancellation-after-claim deterministic without slowing production code.
    let mut input_blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE planning_requests IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *input_blocker)
        .await
        .unwrap();
    let running = worker.clone();
    let task = tokio::spawn(async move { running.take_one().await });
    for _ in 0..200 {
        let state: String =
            sqlx::query_scalar("SELECT state::text FROM generation_jobs WHERE id=$1")
                .bind(cancelled)
                .fetch_one(&pool)
                .await
                .unwrap();
        if state == "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let state: String = sqlx::query_scalar("SELECT state::text FROM generation_jobs WHERE id=$1")
        .bind(cancelled)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        state, "running",
        "the worker must claim the job before cancellation"
    );
    coordinator
        .cancel(owner, cancelled, std::time::SystemTime::now())
        .await
        .unwrap();
    input_blocker.rollback().await.unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(
        coordinator.status(owner, cancelled).await.unwrap().state,
        JobState::Cancelled
    );

    seed_job(
        &pool,
        owner,
        JobFixture {
            index: 3,
            template_id: "heart",
            longitude: 77.5939555,
            latitude: 12.9724454,
            target: 1_300,
            profile_json: r#"{"activity":"walking","profile_version":"walking-v1"}"#,
            algorithm_version: "algorithm-v1",
            deadline: "1 minute",
        },
    )
    .await;
    sqlx::raw_sql(
        "CREATE FUNCTION fail_candidate_publication() RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN RAISE EXCEPTION 'injected candidate persistence failure'; END;
         $$;
         CREATE TRIGGER injected_candidate_persistence_failure
         BEFORE INSERT ON route_candidates
         FOR EACH ROW EXECUTE FUNCTION fail_candidate_publication();",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        worker.take_one().await,
        Err(WorkerError::JobStateUnavailable)
    );
    sqlx::raw_sql(
        "DROP TRIGGER injected_candidate_persistence_failure ON route_candidates;
         DROP FUNCTION fail_candidate_publication();",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM route_candidate_snapshots")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    drop_database(admin, pool, &database_name).await;
}
