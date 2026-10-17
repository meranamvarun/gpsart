use gpsart_config::WorkloadLimits;
use gpsart_persistence::jobs::{
    Completion, JobError, JobOutcomeCode, JobStage, PostgresJobCoordinator, RouteJobSubmission,
};
use gpsart_persistence::routes::{
    CandidateSnapshot, PostgresRouteSnapshotStore, RouteSnapshotError, RouteSnapshotStore,
    SnapshotIdentity,
};
use gpsart_persistence::{
    GuestSessionStore, MIGRATOR, MigrationStatus, PostgresGuestSessionStore,
    PostgresResolvedStartStore, ResolvedStartRecord, ResolvedStartStore, migration_status,
};
use sqlx::{Connection, Executor, PgConnection, PgPool, Row, postgres::PgConnectOptions};
use std::time::{Duration, SystemTime};
use std::{
    env,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};
use uuid::Uuid;

mod support;

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

async fn apply_embedded_migrations_through(pool: &PgPool, maximum_version: i64) {
    sqlx::raw_sql(
        "CREATE TABLE _sqlx_migrations (version bigint PRIMARY KEY, description text NOT NULL, installed_on timestamptz NOT NULL DEFAULT now(), success boolean NOT NULL, checksum bytea NOT NULL, execution_time bigint NOT NULL)",
    )
    .execute(pool)
    .await
    .unwrap();
    for migration in MIGRATOR
        .iter()
        .filter(|migration| migration.version <= maximum_version)
    {
        sqlx::raw_sql(migration.sql.as_ref())
            .execute(pool)
            .await
            .expect("embedded historical migration applies");
        sqlx::query(
            "INSERT INTO _sqlx_migrations(version, description, success, checksum, execution_time) VALUES ($1, $2, true, $3, 0)",
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .bind(migration.checksum.as_ref())
        .execute(pool)
        .await
        .unwrap();
    }
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
          map_data_version, algorithm_version, return_to_start, expires_at
        ) VALUES (
          '20000000-0000-0000-0000-000000000001',
          '00000000-0000-0000-0000-000000000001',
          '10000000-0000-0000-0000-000000000001', 1, 'request-one',
          decode(repeat('01', 32), 'hex'),
          ST_SetSRID(ST_MakePoint(77.1, 28.6), 4326), 5000, 250,
          '{"activity":"walking"}'::jsonb, 'walking-v1', 'coverage-v1', 'heart', '1',
          'map-v1', 'algorithm-v1', true, now() + interval '1 day'
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
async fn claimed_job_input_is_exactly_versioned_and_lease_fenced() {
    let Some((admin, pool, database_name)) = isolated_database("claimed_job_input").await else {
        return;
    };
    seed_owned_request(&pool).await;
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let now = SystemTime::now();
    let lease = coordinator
        .claim(Uuid::new_v4(), now, Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();

    let input = coordinator.load_claimed_input(&lease, now).await.unwrap();
    assert_eq!(
        input.owner,
        Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
    );
    assert_eq!(input.request_revision, 1);
    assert_eq!(input.confirmed_longitude, 77.1);
    assert_eq!(input.confirmed_latitude, 28.6);
    assert_eq!(input.requested_distance_metres, 5_000);
    assert_eq!(input.distance_tolerance_metres, 250);
    assert!(input.return_to_start);
    assert_eq!(input.activity_profile_version, "walking-v1");
    assert_eq!(input.coverage_policy_version, "coverage-v1");
    assert_eq!(input.template_id, "heart");
    assert_eq!(input.template_version, 1);
    assert_eq!(input.map_data_version, "map-v1");
    assert_eq!(input.algorithm_version, "algorithm-v1");

    let stale = gpsart_persistence::jobs::JobLease {
        worker_id: Uuid::new_v4(),
        ..lease
    };
    assert_eq!(
        coordinator.load_claimed_input(&stale, now).await,
        Err(JobError::LeaseLost)
    );
    drop_database(admin, pool, &database_name).await;
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
        "job_attempts", "route_candidates", "route_candidate_snapshots", "cleanup_runs",
    ])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tables, 8);

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
            versions: vec![2, 3, 4, 5, 6]
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
async fn job_status_migration_upgrades_populated_v4_rows_without_inventing_outcomes() {
    let Some((admin, pool, database_name)) = isolated_database("status_upgrade").await else {
        return;
    };
    apply_embedded_migrations_through(&pool, 4).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(
        r#"
        INSERT INTO guest_sessions (id, token_hash, expires_at, cleanup_after) VALUES
          ('00000000-0000-0000-0000-000000000001', decode(repeat('01', 32), 'hex'), now() + interval '1 day', now() + interval '2 days');
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
          activity_profile, activity_profile_version, coverage_policy_version, template_id,
          template_version, map_data_version, algorithm_version, expires_at
        ) VALUES (
          '20000000-0000-0000-0000-000000000001',
          '00000000-0000-0000-0000-000000000001',
          '10000000-0000-0000-0000-000000000001', 7, 'legacy-request',
          decode(repeat('01', 32), 'hex'),
          ST_SetSRID(ST_MakePoint(77.1, 28.6), 4326), 5000, 250,
          '{"activity":"walking"}'::jsonb, 'walking-v1', 'coverage-v1', 'heart', '1',
          'map-v1', 'algorithm-v1', now() + interval '1 day'
        );
        INSERT INTO generation_jobs (
          id, guest_session_id, planning_request_id, state, stage, queued_at, started_at,
          deadline_at, completed_at
        ) VALUES
          ('30000000-0000-0000-0000-000000000001',
           '00000000-0000-0000-0000-000000000001',
           '20000000-0000-0000-0000-000000000001', 'succeeded', 'comparing_routes',
           now() - interval '2 minutes', now() - interval '119 seconds',
           now() + interval '1 minute', now() - interval '1 minute'),
          ('30000000-0000-0000-0000-000000000002',
           '00000000-0000-0000-0000-000000000001',
           '20000000-0000-0000-0000-000000000001', 'failed', 'checking_roads',
           now() - interval '2 minutes', now() - interval '119 seconds',
           now() + interval '1 minute', now() - interval '1 minute');
        INSERT INTO route_candidates (
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
        );
        INSERT INTO generation_job_outcomes (generation_job_id, outcome, public_code) VALUES
          ('30000000-0000-0000-0000-000000000001', 'succeeded', 'succeeded'),
          ('30000000-0000-0000-0000-000000000002', 'failed', NULL);
        "#,
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    MIGRATOR
        .run(&pool)
        .await
        .expect("v4 to current upgrade succeeds");
    let historical_finish: Option<bool> = sqlx::query_scalar(
        "SELECT return_to_start FROM planning_requests WHERE id='20000000-0000-0000-0000-000000000001'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        historical_finish, None,
        "the migration must not invent a finish preference for historical requests"
    );
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let owner = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let succeeded = coordinator
        .status(
            owner,
            Uuid::parse_str("30000000-0000-0000-0000-000000000001").unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(succeeded.request_revision, 7);
    assert_eq!(succeeded.stage, JobStage::ComparingRoutes);
    assert_eq!(
        succeeded.result_ids,
        vec![Uuid::parse_str("40000000-0000-0000-0000-000000000001").unwrap()]
    );
    let failed = coordinator
        .status(
            owner,
            Uuid::parse_str("30000000-0000-0000-0000-000000000002").unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed.outcome.unwrap().code, JobOutcomeCode::LegacyUnknown);
    assert!(
        sqlx::query("UPDATE generation_job_outcomes SET result_ids='{}' WHERE generation_job_id='30000000-0000-0000-0000-000000000001'")
            .execute(&pool)
            .await
            .is_err(),
        "published result IDs must become immutable during upgrade"
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
async fn confirmed_start_store_feeds_owned_job_submission() {
    let Some((admin, pool, database_name)) = isolated_database("confirmed_start_job").await else {
        return;
    };
    MIGRATOR.run(&pool).await.expect("migrations apply");
    let owner = Uuid::new_v4();
    let start_id = Uuid::new_v4();
    sqlx::query("INSERT INTO guest_sessions (id, token_hash, expires_at, cleanup_after) VALUES ($1, decode(repeat('11', 32), 'hex'), now() + interval '1 day', now() + interval '2 days')")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    let now = SystemTime::now();
    PostgresResolvedStartStore::new(pool.clone())
        .insert_resolved_start(ResolvedStartRecord {
            id: start_id,
            guest_session_id: owner,
            requested_longitude: 10.0,
            requested_latitude: 10.0,
            confirmed_longitude: 10.0001,
            confirmed_latitude: 10.0001,
            profile_version: "walking-v1".into(),
            coverage_policy_version: "coverage-v1".into(),
            map_data_version: "map-v1".into(),
            created_at: now,
            expires_at: now + Duration::from_secs(300),
        })
        .await
        .unwrap();
    let job = PostgresJobCoordinator::new(pool.clone())
        .submit(
            owner,
            &RouteJobSubmission {
                resolved_start_id: start_id,
                revision: 1,
                idempotency_key: "resolved-flow".into(),
                input_hash: [9; 32],
                requested_distance_metres: 5_000,
                distance_tolerance_metres: 250,
                return_to_start: false,
                activity_profile_json: r#"{"activity":"walking","profile_version":"walking-v1"}"#
                    .into(),
                activity_profile_version: "walking-v1".into(),
                coverage_policy_version: "coverage-v1".into(),
                template_id: "heart".into(),
                template_version: "1".into(),
                map_data_version: "map-v1".into(),
                algorithm_version: "algorithm-v1".into(),
                expires_at: now + Duration::from_secs(3_600),
                deadline_at: now + Duration::from_secs(60),
                max_attempts: 3,
            },
        )
        .await
        .unwrap();
    let stored: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE id=$1 AND guest_session_id=$2)",
    )
    .bind(job)
    .bind(owner)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(stored);
    let return_to_start: Option<bool> = sqlx::query_scalar(
        "SELECT return_to_start FROM planning_requests WHERE id=(SELECT planning_request_id FROM generation_jobs WHERE id=$1)",
    )
    .bind(job)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(return_to_start, Some(false));
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let make_submission = |key: &str, hash: u8| RouteJobSubmission {
        resolved_start_id: start_id,
        revision: i32::from(hash),
        idempotency_key: key.into(),
        input_hash: [hash; 32],
        requested_distance_metres: 5_000,
        distance_tolerance_metres: 250,
        return_to_start: false,
        activity_profile_json: r#"{"activity":"walking","profile_version":"walking-v1"}"#.into(),
        activity_profile_version: "walking-v1".into(),
        coverage_policy_version: "coverage-v1".into(),
        template_id: "heart".into(),
        template_version: "1".into(),
        map_data_version: "map-v1".into(),
        algorithm_version: "algorithm-v1".into(),
        expires_at: SystemTime::now() + Duration::from_secs(3_600),
        deadline_at: SystemTime::now() + Duration::from_secs(60),
        max_attempts: 3,
    };
    let second = make_submission("second-active", 2);
    let second_id = coordinator.submit(owner, &second).await.unwrap();
    assert_eq!(coordinator.submit(owner, &second).await.unwrap(), second_id);
    assert_eq!(
        coordinator
            .submit(owner, &make_submission("new-key-cannot-bypass", 3))
            .await,
        Err(JobError::RateLimited {
            retry_after_seconds: 60,
        }),
        "new idempotency keys cannot bypass the per-guest active-job limit",
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn cleanup_is_batched_and_never_deletes_work_behind_an_active_lease() {
    let Some((admin, pool, database_name)) = isolated_database("retention_cleanup").await else {
        return;
    };
    MIGRATOR.run(&pool).await.expect("migrations apply");
    sqlx::raw_sql(
        r#"
        INSERT INTO guest_sessions (id, token_hash, created_at, expires_at, cleanup_after)
        VALUES ('00000000-0000-0000-0000-000000000011', decode(repeat('31',32),'hex'), now()-interval '2 days', now()+interval '1 day', now()+interval '2 days');
        INSERT INTO resolved_starts (id, guest_session_id, requested_pin, confirmed_start, profile_version, coverage_policy_version, map_data_version, created_at, expires_at)
        VALUES ('10000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000011',ST_SetSRID(ST_MakePoint(10,10),4326),ST_SetSRID(ST_MakePoint(10,10),4326),'walking-v1','coverage-v1','map-v1',now()-interval '2 days',now()-interval '1 day');
        INSERT INTO planning_requests (id, guest_session_id, resolved_start_id, revision, idempotency_key, input_hash, requested_pin, requested_distance_metres, distance_tolerance_metres, activity_profile, activity_profile_version, coverage_policy_version, template_id, template_version, map_data_version, algorithm_version, return_to_start, created_at, expires_at)
        VALUES ('20000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000011','10000000-0000-0000-0000-000000000011',1,'expired',decode(repeat('41',32),'hex'),ST_SetSRID(ST_MakePoint(10,10),4326),5000,250,'{}','walking-v1','coverage-v1','heart','1','map-v1','algorithm-v1',true,now()-interval '2 days',now()-interval '1 day');
        INSERT INTO generation_jobs (id, guest_session_id, planning_request_id, state, stage, queued_at, started_at, deadline_at, lease_owner, lease_expires_at, max_attempts)
        VALUES ('30000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000011','20000000-0000-0000-0000-000000000011','running','fitting_design',now()-interval '2 days',now()-interval '2 days',now()+interval '1 hour','50000000-0000-0000-0000-000000000011',now()+interval '5 minutes',3);
        INSERT INTO route_candidates (id, guest_session_id, generation_job_id, candidate_version, start_point, end_point, canonical_route, distance_metres, map_data_version, algorithm_version, created_at, expires_at)
        VALUES ('40000000-0000-0000-0000-000000000011','00000000-0000-0000-0000-000000000011','30000000-0000-0000-0000-000000000011',1,ST_SetSRID(ST_MakePoint(10,10),4326),ST_SetSRID(ST_MakePoint(10.1,10.1),4326),ST_SetSRID(ST_MakeLine(ST_MakePoint(10,10),ST_MakePoint(10.1,10.1)),4326),5000,'map-v1','algorithm-v1',now()-interval '2 days',now()-interval '1 day');
        "#,
    ).execute(&pool).await.unwrap();
    let store = PostgresGuestSessionStore::new(pool.clone());
    let protected = store
        .cleanup_expired_sessions(SystemTime::now())
        .await
        .unwrap();
    assert_eq!(protected.deleted_jobs, 0);
    assert_eq!(protected.deleted_candidates, 0);
    sqlx::query("UPDATE generation_jobs SET lease_expires_at=now()-interval '1 second' WHERE id='30000000-0000-0000-0000-000000000011'")
        .execute(&pool).await.unwrap();
    let deleted = store
        .cleanup_expired_sessions(SystemTime::now())
        .await
        .unwrap();
    assert_eq!(deleted.deleted_jobs, 1);
    assert_eq!(deleted.deleted_candidates, 1);
    assert_eq!(deleted.deleted_starts, 1);
    assert!(!deleted.retry_required);

    sqlx::raw_sql(
        r#"
        INSERT INTO guest_sessions (id, token_hash, created_at, expires_at, cleanup_after)
        VALUES ('00000000-0000-0000-0000-000000000099', decode(repeat('99',32),'hex'), now()-interval '3 days', now()-interval '2 days', now()-interval '1 day');
        INSERT INTO resolved_starts (id, guest_session_id, requested_pin, confirmed_start, profile_version, coverage_policy_version, map_data_version, created_at, expires_at)
        SELECT md5('start-' || value::text)::uuid, '00000000-0000-0000-0000-000000000099', ST_SetSRID(ST_MakePoint(10,10),4326), ST_SetSRID(ST_MakePoint(10,10),4326), 'walking-v1', 'coverage-v1', 'map-v1', now()-interval '2 days', now()-interval '1 day' FROM generate_series(1,101) value;
        INSERT INTO planning_requests (id, guest_session_id, resolved_start_id, revision, idempotency_key, input_hash, requested_pin, requested_distance_metres, distance_tolerance_metres, activity_profile, activity_profile_version, coverage_policy_version, template_id, template_version, map_data_version, algorithm_version, return_to_start, created_at, expires_at)
        SELECT md5('request-' || value::text)::uuid, '00000000-0000-0000-0000-000000000099', md5('start-' || value::text)::uuid, value, 'batch-' || value::text, decode(md5(value::text) || md5(value::text),'hex'), ST_SetSRID(ST_MakePoint(10,10),4326), 5000, 250, '{}', 'walking-v1', 'coverage-v1', 'heart', '1', 'map-v1', 'algorithm-v1', true, now()-interval '2 days', now()-interval '1 day' FROM generate_series(1,101) value;
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let first_family_batch = store
        .cleanup_expired_sessions(SystemTime::now())
        .await
        .unwrap();
    assert_eq!(first_family_batch.deleted_sessions, 0);
    assert!(first_family_batch.retry_required);
    let remaining_families: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM planning_requests WHERE guest_session_id='00000000-0000-0000-0000-000000000099'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        remaining_families, 1,
        "session cascades must not bypass the 100-family batch cap"
    );
    let final_family_batch = store
        .cleanup_expired_sessions(SystemTime::now())
        .await
        .unwrap();
    assert_eq!(final_family_batch.deleted_sessions, 1);

    sqlx::raw_sql(
        "INSERT INTO guest_sessions (id, token_hash, created_at, expires_at, cleanup_after) SELECT md5(value::text)::uuid, decode(lpad(to_hex(value),64,'0'),'hex'), now()-interval '3 days', now()-interval '2 days', now()-interval '1 day' FROM generate_series(1,101) value",
    )
    .execute(&pool)
    .await
    .unwrap();
    let first_batch = store
        .cleanup_expired_sessions(SystemTime::now())
        .await
        .unwrap();
    assert_eq!(first_batch.deleted_sessions, 100);
    assert!(first_batch.retry_required);
    let final_batch = store
        .cleanup_expired_sessions(SystemTime::now())
        .await
        .unwrap();
    assert_eq!(final_batch.deleted_sessions, 1);
    assert!(!final_batch.retry_required);
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn provider_reservation_orders_scheduling_before_cancellation() {
    let Some((admin, pool, database_name)) = isolated_database("provider_cancel_order").await
    else {
        return;
    };
    seed_owned_request(&pool).await;
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let owner = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let now = SystemTime::now();
    let lease = coordinator
        .claim(Uuid::new_v4(), now, Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    let permit = coordinator
        .begin_provider_call(&lease, now, Duration::from_secs(5))
        .await
        .unwrap()
        .expect("running lease receives a provider permit");
    let cancelling = {
        let coordinator = coordinator.clone();
        tokio::spawn(async move { coordinator.cancel(owner, lease.job_id, now).await })
    };
    tokio::pin!(cancelling);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut cancelling)
            .await
            .is_err(),
        "cancellation must wait until the scheduling permit is committed"
    );
    permit.commit().await.unwrap();
    assert_eq!(
        cancelling.await.unwrap().unwrap(),
        gpsart_persistence::jobs::JobState::Cancelled
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn provider_call_budget_is_durable_and_hard_capped() {
    let Some((admin, pool, database_name)) = isolated_database("provider_budget").await else {
        return;
    };
    seed_owned_request(&pool).await;
    let limits = WorkloadLimits {
        max_provider_requests: 1,
        ..WorkloadLimits::default()
    };
    let coordinator = PostgresJobCoordinator::with_limits(pool.clone(), limits);
    let now = SystemTime::now();
    let lease = coordinator
        .claim(Uuid::new_v4(), now, Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    coordinator
        .begin_provider_call(&lease, now, Duration::from_secs(1))
        .await
        .unwrap()
        .unwrap()
        .commit()
        .await
        .unwrap();
    assert!(
        coordinator
            .begin_provider_call(&lease, now, Duration::from_secs(1))
            .await
            .unwrap()
            .is_none(),
        "a new worker or retry cannot reset the durable provider-call budget"
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn candidate_publication_requires_request_versions_and_the_deadline() {
    let Some((admin, pool, database_name)) = isolated_database("candidate_versions").await else {
        return;
    };
    seed_owned_request(&pool).await;
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let now = SystemTime::now();
    let lease = coordinator
        .claim(Uuid::new_v4(), now, Duration::from_secs(1_200))
        .await
        .unwrap()
        .unwrap();
    let candidate = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO route_candidates (
          id, guest_session_id, generation_job_id, candidate_version, start_point,
          end_point, canonical_route, distance_metres, map_data_version,
          algorithm_version, expires_at
        ) VALUES (
          $1, '00000000-0000-0000-0000-000000000001', $2, 1,
          ST_SetSRID(ST_MakePoint(10.0, 10.0), 4326),
          ST_SetSRID(ST_MakePoint(10.1, 10.1), 4326),
          ST_SetSRID(ST_MakeLine(ST_MakePoint(10.0, 10.0), ST_MakePoint(10.1, 10.1)), 4326),
          5000, 'map-v1', 'wrong-algorithm', now() + interval '1 day'
        )"#,
    )
    .bind(candidate)
    .bind(lease.job_id)
    .execute(&pool)
    .await
    .unwrap();
    let completion = Completion::Succeeded {
        candidate_ids: vec![candidate],
        budget_exhausted: false,
    };
    assert_eq!(
        coordinator.complete(&lease, &completion, now).await,
        Err(JobError::InvalidInput)
    );
    assert_eq!(
        coordinator
            .complete(&lease, &completion, now + Duration::from_secs(660))
            .await
            .unwrap(),
        gpsart_persistence::jobs::JobState::TimedOut
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn complete_candidate_snapshot_round_trips_immutably_and_owner_scoped() {
    let Some((admin, pool, database_name)) = isolated_database("route_snapshot").await else {
        return;
    };
    seed_owned_request(&pool).await;
    let owner = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let route_id = Uuid::parse_str("40000000-0000-4000-8000-000000000001").unwrap();
    let now = SystemTime::now();
    let source = support::scored_candidate("map-v1", 417).await;
    let snapshot = CandidateSnapshot::from_scored(
        &source,
        SnapshotIdentity {
            id: route_id,
            generation_job_id: Uuid::parse_str("30000000-0000-0000-0000-000000000001").unwrap(),
            candidate_version: 1,
            request_revision: 1,
            generated_at_epoch_seconds: now
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        },
    )
    .unwrap();
    let store = PostgresRouteSnapshotStore::new(pool.clone());
    let mut wrong_request_revision = snapshot.clone();
    wrong_request_revision.request_revision = 2;
    assert_eq!(
        store
            .publish(
                owner,
                &wrong_request_revision,
                &source,
                now + Duration::from_secs(86_400),
            )
            .await,
        Err(RouteSnapshotError::InvalidSnapshot)
    );
    let unpublished: i64 = sqlx::query_scalar("SELECT count(*) FROM route_candidates WHERE id=$1")
        .bind(route_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        unpublished, 0,
        "a snapshot with stale request pins is not published"
    );
    store
        .publish(owner, &snapshot, &source, now + Duration::from_secs(86_400))
        .await
        .unwrap();
    assert_eq!(store.get(owner, route_id).await.unwrap(), snapshot);
    assert_eq!(
        store.get(Uuid::new_v4(), route_id).await,
        Err(RouteSnapshotError::NotFound)
    );
    assert!(
        sqlx::query(
            "UPDATE route_candidate_snapshots SET snapshot_json = '{}' WHERE candidate_id=$1"
        )
        .bind(route_id)
        .execute(&pool)
        .await
        .is_err()
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn candidate_snapshot_and_success_outcome_publish_atomically_under_the_lease() {
    let Some((admin, pool, database_name)) = isolated_database("atomic_worker_success").await
    else {
        return;
    };
    seed_owned_request(&pool).await;
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let owner = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let now = SystemTime::now();
    let lease = coordinator
        .claim(Uuid::new_v4(), now, Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    let source = support::scored_candidate("map-v1", 417).await;
    let snapshot = CandidateSnapshot::from_scored(
        &source,
        SnapshotIdentity {
            id: Uuid::new_v4(),
            generation_job_id: lease.job_id,
            candidate_version: 1,
            request_revision: 1,
            generated_at_epoch_seconds: now
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        },
    )
    .unwrap();

    let stale = gpsart_persistence::jobs::JobLease {
        worker_id: Uuid::new_v4(),
        ..lease.clone()
    };
    assert_eq!(
        coordinator
            .publish_success(
                &stale,
                owner,
                &snapshot,
                &source,
                now + Duration::from_secs(86_400),
                now,
            )
            .await,
        Err(JobError::LeaseLost),
    );
    let unpublished: i64 = sqlx::query_scalar("SELECT count(*) FROM route_candidates WHERE id=$1")
        .bind(snapshot.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(unpublished, 0);

    assert_eq!(
        coordinator
            .publish_success(
                &lease,
                owner,
                &snapshot,
                &source,
                now + Duration::from_secs(86_400),
                now,
            )
            .await
            .unwrap(),
        gpsart_persistence::jobs::JobState::Succeeded,
    );
    let status = coordinator.status(owner, lease.job_id).await.unwrap();
    assert_eq!(status.result_ids, vec![snapshot.id]);
    assert_eq!(
        PostgresRouteSnapshotStore::new(pool.clone())
            .get(owner, snapshot.id)
            .await
            .unwrap(),
        snapshot
    );
    assert_eq!(
        coordinator
            .publish_success(
                &lease,
                owner,
                &snapshot,
                &source,
                now + Duration::from_secs(86_400),
                now,
            )
            .await,
        Err(JobError::Terminal),
    );
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn job_status_is_owner_scoped_monotonic_and_atomically_classified() {
    let Some((admin, pool, database_name)) = isolated_database("job_status").await else {
        return;
    };
    seed_owned_request(&pool).await;
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let owner = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let foreign = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
    let job_id = Uuid::parse_str("30000000-0000-0000-0000-000000000001").unwrap();
    let queued = coordinator.status(owner, job_id).await.unwrap();
    assert_eq!(queued.state, gpsart_persistence::jobs::JobState::Queued);
    assert_eq!(queued.stage, JobStage::Queued);
    assert_eq!(queued.status_version, 1);
    assert!(queued.outcome.is_none());
    assert_eq!(
        coordinator.status(foreign, job_id).await,
        Err(JobError::NotFound)
    );

    let now = SystemTime::now();
    let lease = coordinator
        .claim(Uuid::new_v4(), now, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let checking = coordinator.status(owner, job_id).await.unwrap();
    assert_eq!(checking.stage, JobStage::CheckingRoads);
    assert_eq!(checking.status_version, 2);
    assert!(checking.started_at.is_some());
    assert_eq!(
        coordinator
            .advance_stage(&lease, now, JobStage::FittingDesign)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        coordinator
            .advance_stage(&lease, now, JobStage::FittingDesign)
            .await
            .unwrap(),
        3
    );
    assert_eq!(
        coordinator
            .advance_stage(&lease, now, JobStage::ComparingRoutes)
            .await
            .unwrap(),
        4
    );
    let recovered = coordinator
        .claim(
            Uuid::new_v4(),
            now + Duration::from_secs(61),
            Duration::from_secs(60),
        )
        .await
        .unwrap()
        .expect("expired running lease is recovered");
    let after_recovery = coordinator.status(owner, job_id).await.unwrap();
    assert_eq!(after_recovery.stage, JobStage::ComparingRoutes);
    assert_eq!(after_recovery.status_version, 5);
    assert_eq!(
        coordinator
            .complete(
                &recovered,
                &Completion::NoMatch {
                    code: JobOutcomeCode::InsufficientVisualMatch,
                },
                now + Duration::from_secs(61),
            )
            .await
            .unwrap(),
        gpsart_persistence::jobs::JobState::NoMatch
    );
    let terminal = coordinator.status(owner, job_id).await.unwrap();
    assert_eq!(terminal.status_version, 6);
    assert_eq!(terminal.stage, JobStage::ComparingRoutes);
    assert_eq!(
        terminal.outcome.unwrap().code,
        JobOutcomeCode::InsufficientVisualMatch
    );
    assert!(terminal.completed_at.is_some());
    assert!(terminal.result_ids.is_empty());
    drop_database(admin, pool, &database_name).await;
}

#[tokio::test]
async fn succeeded_status_freezes_the_exact_atomic_result_set() {
    let Some((admin, pool, database_name)) = isolated_database("frozen_results").await else {
        return;
    };
    seed_owned_request(&pool).await;
    let coordinator = PostgresJobCoordinator::new(pool.clone());
    let owner = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let now = SystemTime::now();
    let lease = coordinator
        .claim(Uuid::new_v4(), now, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    let published = Uuid::parse_str("40000000-0000-4000-8000-000000000001").unwrap();
    sqlx::query(
        r#"INSERT INTO route_candidates (
          id, guest_session_id, generation_job_id, candidate_version, start_point,
          end_point, canonical_route, distance_metres, map_data_version,
          algorithm_version, expires_at
        ) VALUES (
          $1, '00000000-0000-0000-0000-000000000001', $2, 1,
          ST_SetSRID(ST_MakePoint(10.0, 10.0), 4326),
          ST_SetSRID(ST_MakePoint(10.1, 10.1), 4326),
          ST_SetSRID(ST_MakeLine(ST_MakePoint(10.0, 10.0), ST_MakePoint(10.1, 10.1)), 4326),
          5000, 'map-v1', 'algorithm-v1', now() + interval '1 day'
        )"#,
    )
    .bind(published)
    .bind(lease.job_id)
    .execute(&pool)
    .await
    .unwrap();
    coordinator
        .complete(
            &lease,
            &Completion::Succeeded {
                candidate_ids: vec![published],
                budget_exhausted: false,
            },
            now,
        )
        .await
        .unwrap();
    let completed = coordinator.status(owner, lease.job_id).await.unwrap();
    assert_eq!(completed.result_ids, vec![published]);

    let unpublished = Uuid::parse_str("40000000-0000-4000-8000-000000000002").unwrap();
    sqlx::query(
        r#"INSERT INTO route_candidates (
          id, guest_session_id, generation_job_id, candidate_version, start_point,
          end_point, canonical_route, distance_metres, map_data_version,
          algorithm_version, expires_at
        ) VALUES (
          $1, '00000000-0000-0000-0000-000000000001', $2, 2,
          ST_SetSRID(ST_MakePoint(10.0, 10.0), 4326),
          ST_SetSRID(ST_MakePoint(10.2, 10.2), 4326),
          ST_SetSRID(ST_MakeLine(ST_MakePoint(10.0, 10.0), ST_MakePoint(10.2, 10.2)), 4326),
          5100, 'map-v1', 'algorithm-v1', now() + interval '1 day'
        )"#,
    )
    .bind(unpublished)
    .bind(lease.job_id)
    .execute(&pool)
    .await
    .unwrap();
    let reread = coordinator.status(owner, lease.job_id).await.unwrap();
    assert_eq!(reread.status_version, completed.status_version);
    assert_eq!(reread.result_ids, vec![published]);
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
        "INSERT INTO generation_job_outcomes (generation_job_id, outcome, public_code) VALUES ('30000000-0000-0000-0000-000000000001', 'failed', 'attempts_exhausted')",
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
        "UPDATE generation_jobs SET state = 'failed', completed_at = now() WHERE id = '30000000-0000-0000-0000-000000000001'",
    )
    .execute(&mut *matching)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO generation_job_outcomes (generation_job_id, outcome, public_code) VALUES ('30000000-0000-0000-0000-000000000001', 'failed', 'attempts_exhausted')",
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
