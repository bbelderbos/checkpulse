mod config;
mod dashboard;
mod db;
mod hash;
mod ingest;
mod ua;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use config::Config;
use hash::Salt;
use ingest::RateLimiter;
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub salt: Arc<Salt>,
    pub limiter: Arc<RateLimiter>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "checkpulse=info".into()),
        )
        .init();

    let config = Config::from_env();
    let pool = db::connect(&config.database_path).await?;

    let addr = format!("{}:{}", config.bind, config.port);
    let state = AppState {
        pool,
        config: Arc::new(config),
        salt: Arc::new(Salt::new()),
        limiter: Arc::new(RateLimiter::new(120, 60)),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(dashboard::dashboard))
        .route("/script.js", get(ingest::script))
        .route(
            "/api/event",
            post(ingest::ingest).layer(DefaultBodyLimit::max(4096)),
        )
        .route("/health", get(|| async { "ok" }))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("checkpulse listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
