use gpsart_persistence::apply_migrations;

#[tokio::main]
async fn main() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set for the separately authorized migration job");
    if let Err(error) = apply_migrations(&database_url).await {
        eprintln!("database migration failed: {error}");
        std::process::exit(1);
    }
}
