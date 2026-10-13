pub mod config;
pub mod handlers;
pub mod models;
pub mod rate;
pub mod store;
pub mod upstream;

use axum::{
    http::StatusCode,
    routing::{get, post},
    Router,
};
use chrono::Utc;
use config::Config;
use rate::RateLimiter;
use std::sync::Arc;
use std::time::Duration;
use store::Store;
use tokio::sync::Semaphore;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub client: reqwest::Client,
    pub config: Arc<Config>,
    pub rate: Arc<RateLimiter>,
    pub semaphore: Arc<Semaphore>,
}

pub fn build_state(config: Config, store: Arc<Store>) -> AppState {
    AppState {
        store,
        client: build_client(),
        semaphore: Arc::new(Semaphore::new(config.max_upstream_concurrency)),
        rate: Arc::new(RateLimiter::per_minute(config.rate_limit_per_minute)),
        config: Arc::new(config),
    }
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(upstream::UPSTREAM_TIMEOUT)
        .user_agent("avatar-generator/0.1")
        .build()
        .expect("failed to build HTTP client")
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(handlers::healthz))
        .route("/", get(handlers::index))
        .route("/generate", post(handlers::generate))
        .route(
            "/avatar/:id",
            get(handlers::get_avatar).delete(handlers::delete_avatar),
        )
        .route("/download/:id", get(handlers::download))
        .route("/download-all", get(handlers::download_all))
        .route("/clear-all", post(handlers::clear_all))
        .route("/static/*path", get(handlers::static_file))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(60),
        ))
        .with_state(state)
}

async fn cleanup_loop(store: Arc<Store>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        match store.cleanup_expired(Utc::now()) {
            Ok(removed) => tracing::info!(removed, "expired avatars cleaned up"),
            Err(e) => tracing::error!("cleanup failed: {e}"),
        }
    }
}

pub async fn run() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    let store = Arc::new(Store::new(config.data_dir.clone()).expect("failed to init store"));

    let cleanup_store = store.clone();
    let interval = Duration::from_secs(config.cleanup_interval_secs);
    tokio::spawn(async move { cleanup_loop(cleanup_store, interval).await });

    let state = build_state(config.clone(), store);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("failed to bind port");
    tracing::info!(port = config.port, "listening");
    axum::serve(listener, app(state))
        .await
        .expect("server error");
}
