use gpsart_persistence::{
    MIGRATOR,
    jobs::{JobState, PostgresJobCoordinator},
};
use gpsart_worker::production::ProductionWorker;
use sqlx::{
    ConnectOptions, Connection, Executor, PgConnection, PgPool, postgres::PgConnectOptions,
};
use std::{env, path::PathBuf, str::FromStr};
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

#[tokio::test]
async fn production_worker_executes_real_heart_and_text_jobs_without_injected_candidates() {
    let Some((admin, pool, database_name, database_url)) = isolated_database().await else {
        return;
    };
    MIGRATOR.run(&pool).await.unwrap();
    let map_version = "osm-bengaluru-central-pedestrian-pilot-v3:sha256:df80123d5403045f30fdef0db9bf626fb05a4f175f58375ff1da852f6b9eab45";
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
        let start_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        sqlx::query("INSERT INTO resolved_starts (id,guest_session_id,requested_pin,confirmed_start,profile_version,coverage_policy_version,map_data_version,expires_at) VALUES ($1,$2,ST_SetSRID(ST_MakePoint($3,$4),4326),ST_SetSRID(ST_MakePoint($3,$4),4326),'walking-v1','pilot-v1',$5,now()+interval '1 day')")
            .bind(start_id).bind(owner).bind(longitude).bind(latitude).bind(map_version)
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO planning_requests (id,guest_session_id,resolved_start_id,revision,idempotency_key,input_hash,requested_pin,requested_distance_metres,distance_tolerance_metres,activity_profile,activity_profile_version,coverage_policy_version,template_id,template_version,map_data_version,algorithm_version,return_to_start,expires_at) VALUES ($1,$2,$3,$4,$5,decode(repeat('32',32),'hex'),ST_SetSRID(ST_MakePoint($6,$7),4326),$8,$9,'{\"activity\":\"walking\",\"profile_version\":\"walking-v1\"}'::jsonb,'walking-v1','pilot-v1',$10,'1',$11,'algorithm-v1',true,now()+interval '1 day')")
            .bind(request_id).bind(owner).bind(start_id).bind((index + 1) as i32)
            .bind(format!("pipeline-{index}")).bind(longitude).bind(latitude).bind(target)
            .bind(target / 5).bind(template_id).bind(map_version).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO generation_jobs (id,guest_session_id,planning_request_id,deadline_at,max_attempts) VALUES ($1,$2,$3,now()+interval '1 minute',3)")
            .bind(job_id).bind(owner).bind(request_id).execute(&pool).await.unwrap();
    }
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../gpsart-routing/data");
    let worker = ProductionWorker::connect(
        &database_url,
        &data.join("bengaluru-central-osm-v3.json"),
        &data.join("bengaluru-central-osm-v3.sha256"),
    )
    .await
    .unwrap();
    worker.take_one().await.unwrap();
    worker.take_one().await.unwrap();
    let coordinator = PostgresJobCoordinator::connect(&database_url)
        .await
        .unwrap();
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
    let snapshots: i64 = sqlx::query_scalar("SELECT count(*) FROM route_candidate_snapshots")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(snapshots, 2);
    drop_database(admin, pool, &database_name).await;
}
