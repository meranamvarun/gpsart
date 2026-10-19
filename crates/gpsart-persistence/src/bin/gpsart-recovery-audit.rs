use gpsart_persistence::{
    GuestSessionStore, MigrationStatus, PostgresGuestSessionStore, migration_status,
    routes::CandidateSnapshot,
};
use sqlx::{
    PgPool, Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{process::Command, time::SystemTime};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[tokio::main]
async fn main() {
    let result = if std::env::args().nth(1).as_deref() == Some("--repository-age") {
        repository_age().map(|seconds| println!("{seconds}"))
    } else {
        audit().await
    };
    if let Err(error) = result {
        eprintln!("isolated recovery audit failed: {error}");
        std::process::exit(1);
    }
}

fn repository_age() -> Result<i64, Box<dyn std::error::Error>> {
    let output = Command::new("restic")
        .args([
            "snapshots",
            "--latest",
            "1",
            "--tag",
            "gpsart-production",
            "--json",
        ])
        .output()?;
    if !output.status.success() {
        return Err("encrypted backup metadata is unavailable".into());
    }
    let snapshots: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let created_at = snapshots
        .as_array()
        .and_then(|items| items.last())
        .and_then(|snapshot| snapshot.get("time"))
        .and_then(|value| value.as_str())
        .ok_or("encrypted backup has no creation time")?;
    let created_at = OffsetDateTime::parse(created_at, &Rfc3339)?;
    let age = (OffsetDateTime::now_utc() - created_at).whole_seconds();
    if age < 0 {
        return Err("encrypted backup creation time is in the future".into());
    }
    Ok(age)
}

async fn audit() -> Result<(), Box<dyn std::error::Error>> {
    let pool = if let Ok(database_url) = std::env::var("DATABASE_URL") {
        PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await?
    } else {
        let options = PgConnectOptions::new()
            .host(&required("PGHOST")?)
            .port(
                std::env::var("PGPORT")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(5432),
            )
            .username(&required("PGUSER")?)
            .password(&required("PGPASSWORD")?)
            .database(&required("PGDATABASE")?);
        PgPoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await?
    };

    if migration_status(&pool).await? != MigrationStatus::Current {
        return Err("migration checksums are not current".into());
    }

    cleanup_expired(&pool).await?;
    let expired_data_remaining: i64 = sqlx::query_scalar(
        "SELECT
           (SELECT count(*) FROM guest_sessions WHERE cleanup_after <= now()) +
           (SELECT count(*) FROM resolved_starts WHERE expires_at <= now()) +
           (SELECT count(*) FROM planning_requests WHERE expires_at <= now())",
    )
    .fetch_one(&pool)
    .await?;
    if expired_data_remaining != 0 {
        return Err("expiry cleanup left restored private data behind".into());
    }

    let ownership_mismatches: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM route_candidates rc
         JOIN generation_jobs gj ON gj.id=rc.generation_job_id
         WHERE rc.guest_session_id <> gj.guest_session_id",
    )
    .fetch_one(&pool)
    .await?;
    if ownership_mismatches != 0 {
        return Err("authorization ownership isolation is inconsistent".into());
    }

    let snapshot_rows =
        sqlx::query("SELECT snapshot_json FROM route_candidate_snapshots ORDER BY candidate_id")
            .fetch_all(&pool)
            .await?;
    for row in &snapshot_rows {
        let snapshot: CandidateSnapshot = serde_json::from_value(row.try_get("snapshot_json")?)
            .map_err(|_| "an immutable route snapshot is unreadable")?;
        snapshot
            .validate()
            .map_err(|_| "an immutable route snapshot is invalid")?;
    }

    let false_successes: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM generation_jobs gj
         LEFT JOIN generation_job_outcomes outcome ON outcome.generation_job_id=gj.id
         LEFT JOIN LATERAL unnest(outcome.result_ids) result(candidate_id) ON true
         LEFT JOIN route_candidate_snapshots snapshot ON snapshot.candidate_id=result.candidate_id
         WHERE gj.state='succeeded'
           AND (outcome.outcome IS DISTINCT FROM 'succeeded' OR result.candidate_id IS NULL OR snapshot.candidate_id IS NULL)",
    )
    .fetch_one(&pool)
    .await?;
    if false_successes != 0 {
        return Err("a success outcome does not resolve to a readable snapshot".into());
    }

    println!(
        "recovery audit passed: migration_checksums=current snapshot_count={} ownership_mismatches=0 expired_data_remaining=0 false_successes=0",
        snapshot_rows.len()
    );
    pool.close().await;
    Ok(())
}

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name).map_err(|_| format!("{name} must be set").into())
}

async fn cleanup_expired(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let store = PostgresGuestSessionStore::new(pool.clone());
    for _ in 0..1000 {
        let report = store.cleanup_expired_sessions(SystemTime::now()).await?;
        if !report.retry_required {
            return Ok(());
        }
        if report.deleted_sessions == 0
            && report.deleted_jobs == 0
            && report.deleted_candidates == 0
            && report.deleted_starts == 0
        {
            return Err("expiry cleanup made no progress".into());
        }
    }
    Err("expiry cleanup exceeded its bounded batch count".into())
}
