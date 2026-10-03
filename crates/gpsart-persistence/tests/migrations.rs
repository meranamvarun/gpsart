use gpsart_persistence::jobs::{Completion, JobError, PostgresJobCoordinator};
use gpsart_persistence::{MIGRATOR, MigrationStatus, migration_status};
use sqlx::{Connection, Executor, PgConnection, PgPool, Row, postgres::PgConnectOptions};
use std::time::{Duration, SystemTime};
use std::{
    env,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};
use uuid::Uuid;

static DATABASE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

async fn isolated_database(test_name: &str) -> Option<(PgConnection, PgPool, String)> {
    let database_url = match env::var("DATABASE_URL") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("DATABASE_URL is not set; skipping PostGIS migration integration test");
            return None;
        }
    };
    let mut admin = PgConnection::connect(&database_url)
        .await
        .expect("DATABASE_URL must identify an available PostgreSQL administrator database");
    let database_name = format!(
        "gpsart_{}_{}_{}",
        test_name,
        std::process::id(),
        DATABASE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    admin
        .execute(format!("CREATE DATABASE {database_name}").as_str())
        .await
        .expect("test database creation must succeed");
    let options = PgConnectOptions::from_str(&database_url)
        .expect("DATABASE_URL must be a PostgreSQL URL")
        .database(&database_name);
    let pool = PgPool::connect_with(options)
        .await
        .expect("isolated test database must accept connections");
    Some((admin, pool, database_name))
}

async fn drop_database(mut admin: PgConnection, pool: PgPool, database_name: &str) {
    pool.close().await;
    admin
        .execute(format!("DROP DATABASE {database_name} WITH (FORCE)").as_str())
        .await
        .expect("isolated test database must be removable");
}

async fn seed_owned_request(pool: &PgPool) {
    MIGRATOR.run(pool).await.expect("migrations apply");
    sqlx::raw_sql(
        r#"
        INSERT INTO guest_sessions (id, token_hash, expires_at, cleanup_after) VALUES
          ('00000000-0000-0000-0000-000000000001', decode(repeat('01', 32), 'hex'), now() + interval '1 day', now() + interval '2 days'),
          ('00000000-0000-0000-0000-000000000002', decode(repeat('02', 32), 'hex'), now() + interval '1 day', now() + interval '2 days');
        INSERT INTO resolved_starts (
          id, guest_session_id, requested_pin, confirmed_start, profile_version,
          coverage_policy_version, map_data_version, expires_at
        ) VALUES (
          '10000000-0000-0000-0000-000000000001',
          '00000000-0000-0000-0000-000000000001',
          ST_SetSRID(ST_MakePoint(77.1, 28.6), 4326),
          ST_SetSRID(ST_MakePoint(77.1, 28.6), 4326),
          'walking-v1', 'coverage-v1', 'map-v1', now() + interval '1 day'
        );
        INSERT INTO planning_requests (
          id, guest_session_id, resolved_start_id, revision, idempotency_key, input_hash,
          requested_pin, requested_distance_metres, distance_tolerance_metres,
          activity_profile, activity_profile_version, coverage_policy_version, template_id, template_version,
          map_data_version, algorithm_version, expires_at
        ) VALUES (
          '20000000-0000-0000-0000-000000000001',
          '00000000-0000-0000-0000-000000000001',
          '10000000-0000-0000-0000-000000000001', 1, 'request-one',
          decode(repeat('01', 32), 'hex'),
          ST_SetSRID(ST_MakePoint(77.1, 28.6), 4326), 5000, 250,
          '{"activity":"walking"}'::jsonb, 'walking-v1', 'coverage-v1', 'heart', '1',
          'map-v1', 'algorithm-v1', now() + interval '1 day'
        );
        INSERT INTO generation_jobs (
          id, guest_session_id, planning_request_id, deadline_at
        ) VALUES (
          '30000000-0000-0000-0000-000000000001',
          '00000000-0000-0000-0000-000000000001',
          '20000000-0000-0000-0000-000000000001', now() + interval '10 minutes'
        );
        "#,
    )
    .execute(pool)
    .await
    .expect("owned request fixture inserts");
}

#[tokio::test]
async fn clean_postgis_database_applies_every_migration() {
    let Some((admin, pool, database_name)) = isolated_database("clean").await else {
        return;
    };
    MIGRATOR.run(&pool).await.expect("clean migration succeeds");
    assert_eq!(
        migration_status(&pool).await.unwrap(),
        MigrationStatus::Current
    );

    let postgis: String = sqlx::query_scalar("SELECT PostGIS_Version()")
        .fetch_one(&pool)
        .await
        .expect("PostGIS is enabled");
    assert!(!postgis.is_empty());
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = ANY($1)",
    )
    .bind(vec![
        "guest_sessions", "resolved_starts", "planning_requests", "generation_jobs",
        "job_attempts", "route_candidates", "cleanup_runs",
    ])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tables, 7);

    let srids: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT srid::integer FROM geometry_columns WHERE f_table_name = ANY($1)",
    )
    .bind(vec![
        "resolved_starts",
        "planning_requests",
        "route_candidates",
    ])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(srids, vec![4326]);

    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn upgrades_from_the_previous_checked_in_migration() {
    let Some((admin, pool, database_name)) = isolated_database("upgrade").await else {
        return;
    };
    let first = MIGRATOR.iter().next().expect("migration 0001 is embedded");
    sqlx::raw_sql(first.sql.as_ref())
        .execute(&pool)
        .await
        .expect("previous migration applies");
    sqlx::raw_sql(
        "CREATE TABLE _sqlx_migrations (version bigint PRIMARY KEY, description text NOT NULL, installed_on timestamptz NOT NULL DEFAULT now(), success boolean NOT NULL, checksum bytea NOT NULL, execution_time bigint NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO guest_sessions (id, token_hash, expires_at, cleanup_after) VALUES ('00000000-0000-0000-0000-000000000099', decode('abcd', 'hex'), now() + interval '1 day', now() + interval '2 days')")
        .execute(&pool)
        .await
        .expect("legacy database accepts a non-SHA-256 token hash");
    sqlx::query(
        "INSERT INTO _sqlx_migrations(version, description, success, checksum, execution_time) VALUES ($1, $2, true, $3, 0)",
    )
    .bind(first.version)
    .bind(first.description.as_ref())
    .bind(first.checksum.as_ref())
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        migration_status(&pool).await.unwrap(),
        MigrationStatus::Pending {
            versions: vec![2, 3, 4]
        }
    );
    MIGRATOR
        .run(&pool)
        .await
        .expect("upgrade migration succeeds");
    assert_eq!(
        migration_status(&pool).await.unwrap(),
        MigrationStatus::Current
    );
    let legacy_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM guest_sessions WHERE id = '00000000-0000-0000-0000-000000000099'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        legacy_rows, 0,
        "invalid legacy credentials fail closed during upgrade"
    );
    let state_labels: Vec<String> = sqlx::query(
        "SELECT enumlabel FROM pg_enum JOIN pg_type ON pg_type.oid = pg_enum.enumtypid WHERE typname = 'generation_job_state' ORDER BY enumsortorder",
    )
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.get("enumlabel"))
    .collect();
    assert_eq!(
        state_labels,
        [
            "queued",
            "running",
            "succeeded",
            "no_match",
            "failed",
            "cancelled",
            "timed_out"
        ]
    );

    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn startup_status_rejects_missing_and_incompatible_migrations() {
    let Some((admin, pool, database_name)) = isolated_database("status").await else {
        return;
    };
    assert_eq!(
        migration_status(&pool).await.unwrap(),
        MigrationStatus::Missing
    );
    MIGRATOR.run(&pool).await.unwrap();
    sqlx::query("UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') WHERE version = 1")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        migration_status(&pool).await.unwrap(),
        MigrationStatus::Incompatible { version: 1 }
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn ownership_is_enforced_across_requests_and_candidates() {
    let Some((admin, pool, database_name)) = isolated_database("ownership").await else {
        return;
    };
    seed_owned_request(&pool).await;

    let cross_owner_request = sqlx::query(
        r#"INSERT INTO planning_requests (
          id, guest_session_id, resolved_start_id, revision, idempotency_key, input_hash,
          requested_pin, requested_distance_metres, distance_tolerance_metres,
          activity_profile, activity_profile_version, coverage_policy_version, template_id, template_version,
          map_data_version, algorithm_version, expires_at
        ) VALUES (
          '20000000-0000-0000-0000-000000000002',
          '00000000-0000-0000-0000-000000000002',
          '10000000-0000-0000-0000-000000000001', 1, 'cross-owner',
          decode(repeat('02', 32), 'hex'),
          ST_SetSRID(ST_MakePoint(77.1, 28.6), 4326), 5000, 250,
          '{}'::jsonb, 'walking-v1', 'coverage-v1', 'heart', '1', 'map-v1', 'algorithm-v1',
          now() + interval '1 day'
        )"#,
    )
    .execute(&pool)
    .await;
    assert!(cross_owner_request.is_err());

    let cross_owner_candidate = sqlx::query(
        r#"INSERT INTO route_candidates (
          id, guest_session_id, generation_job_id, candidate_version, start_point,
          end_point, canonical_route, distance_metres, map_data_version,
          algorithm_version, expires_at
        ) VALUES (
          '40000000-0000-0000-0000-000000000001',
          '00000000-0000-0000-0000-000000000002',
          '30000000-0000-0000-0000-000000000001', 1,
          ST_SetSRID(ST_MakePoint(77.1, 28.6), 4326),
          ST_SetSRID(ST_MakePoint(77.2, 28.7), 4326),
          ST_SetSRID(ST_MakeLine(ST_MakePoint(77.1, 28.6), ST_MakePoint(77.2, 28.7)), 4326),
          5000, 'map-v1', 'algorithm-v1', now() + interval '1 day'
        )"#,
    )
    .execute(&pool)
    .await;
    assert!(cross_owner_candidate.is_err());

    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn concurrent_claim_and_cancellation_are_atomic() {
    let Some((admin, pool, database_name)) = isolated_database("job_lease").await else {
        return;
    };
    seed_owned_request(&pool).await;
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let now = SystemTime::now();
    let worker_one = Uuid::new_v4();
    let worker_two = Uuid::new_v4();
    let (one, two) = tokio::join!(
        coordinator.claim(worker_one, now, Duration::from_secs(30)),
        coordinator.claim(worker_two, now, Duration::from_secs(30)),
    );
    let leases = [one.unwrap(), two.unwrap()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(leases.len(), 1, "SKIP LOCKED permits one valid lease owner");
    let lease = leases.into_iter().next().unwrap();
    let owner = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    assert_eq!(
        coordinator.cancel(owner, lease.job_id, now).await.unwrap(),
        gpsart_persistence::jobs::JobState::Cancelled
    );
    assert_eq!(
        coordinator
            .complete(&lease, &Completion::TimedOut, now)
            .await,
        Err(JobError::Terminal)
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn terminal_jobs_require_exactly_one_matching_outcome() {
    let Some((admin, pool, database_name)) = isolated_database("outcomes").await else {
        return;
    };
    seed_owned_request(&pool).await;

    let mut active_with_outcome = pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO generation_job_outcomes (generation_job_id, outcome) VALUES ('30000000-0000-0000-0000-000000000001', 'succeeded')",
    )
    .execute(&mut *active_with_outcome)
    .await
    .unwrap();
    assert!(active_with_outcome.commit().await.is_err());

    let mut terminal_without_outcome = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE generation_jobs SET state = 'failed', completed_at = now() WHERE id = '30000000-0000-0000-0000-000000000001'",
    )
    .execute(&mut *terminal_without_outcome)
    .await
    .unwrap();
    assert!(terminal_without_outcome.commit().await.is_err());

    let mut matching = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE generation_jobs SET state = 'succeeded', completed_at = now() WHERE id = '30000000-0000-0000-0000-000000000001'",
    )
    .execute(&mut *matching)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO generation_job_outcomes (generation_job_id, outcome) VALUES ('30000000-0000-0000-0000-000000000001', 'succeeded')",
    )
    .execute(&mut *matching)
    .await
    .unwrap();
    matching
        .commit()
        .await
        .expect("matching terminal outcome commits");

    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn immutable_snapshots_and_version_uniqueness_are_enforced() {
    let Some((admin, pool, database_name)) = isolated_database("immutable").await else {
        return;
    };
    seed_owned_request(&pool).await;

    assert!(
        sqlx::query(
            "UPDATE planning_requests SET requested_distance_metres = 6000 WHERE id = '20000000-0000-0000-0000-000000000001'",
        )
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            r#"INSERT INTO planning_requests (
              id, guest_session_id, resolved_start_id, revision, idempotency_key, input_hash,
              requested_pin, requested_distance_metres, distance_tolerance_metres,
              activity_profile, activity_profile_version, coverage_policy_version, template_id, template_version,
              map_data_version, algorithm_version, expires_at
            ) SELECT
              '20000000-0000-0000-0000-000000000003', guest_session_id,
              resolved_start_id, 2, idempotency_key, decode(repeat('03', 32), 'hex'),
              requested_pin, requested_distance_metres, distance_tolerance_metres,
              activity_profile, activity_profile_version, coverage_policy_version, template_id, template_version,
              map_data_version, algorithm_version, expires_at
            FROM planning_requests WHERE id = '20000000-0000-0000-0000-000000000001'"#,
        )
        .execute(&pool)
        .await
        .is_err()
    );

    sqlx::query(
        r#"INSERT INTO route_candidates (
          id, guest_session_id, generation_job_id, candidate_version, start_point,
          end_point, canonical_route, distance_metres, map_data_version,
          algorithm_version, expires_at
        ) VALUES (
          '40000000-0000-0000-0000-000000000001',
          '00000000-0000-0000-0000-000000000001',
          '30000000-0000-0000-0000-000000000001', 1,
          ST_SetSRID(ST_MakePoint(77.1, 28.6), 4326),
          ST_SetSRID(ST_MakePoint(77.2, 28.7), 4326),
          ST_SetSRID(ST_MakeLine(ST_MakePoint(77.1, 28.6), ST_MakePoint(77.2, 28.7)), 4326),
          5000, 'map-v1', 'algorithm-v1', now() + interval '1 day'
        )"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        sqlx::query(
            "UPDATE route_candidates SET distance_metres = 5001 WHERE id = '40000000-0000-0000-0000-000000000001'",
        )
        .execute(&pool)
        .await
        .is_err()
    );
    assert!(
        sqlx::query(
            r#"INSERT INTO route_candidates (
              id, guest_session_id, generation_job_id, candidate_version, start_point,
              end_point, canonical_route, distance_metres, map_data_version,
              algorithm_version, expires_at
            ) SELECT
              '40000000-0000-0000-0000-000000000002', guest_session_id,
              generation_job_id, candidate_version, start_point, end_point, canonical_route,
              distance_metres, map_data_version, algorithm_version, expires_at
            FROM route_candidates WHERE id = '40000000-0000-0000-0000-000000000001'"#,
        )
        .execute(&pool)
        .await
        .is_err()
    );

    drop_database(admin, pool, &database_name).await;
}
