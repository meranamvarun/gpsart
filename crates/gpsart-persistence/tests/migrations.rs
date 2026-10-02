use gpsart_persistence::{MIGRATOR, MigrationStatus, migration_status};
use sqlx::{Connection, Executor, PgConnection, PgPool, Row, postgres::PgConnectOptions};
use std::{
    env,
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

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
        MigrationStatus::Pending { versions: vec![2] }
    );
    MIGRATOR
        .run(&pool)
        .await
        .expect("upgrade migration succeeds");
    assert_eq!(
        migration_status(&pool).await.unwrap(),
        MigrationStatus::Current
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
