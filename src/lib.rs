pub mod config;
pub mod handlers;
pub mod models;
pub mod rate;
pub mod security;
pub mod store;
pub mod upstream;

use crate::security::SecurityHeadersLayer;
use axum::{
    http::StatusCode,
    routing::{get, post},
    Router,
};
use config::Config;
use rate::RateLimiter;
use std::sync::Arc;
use std::time::Duration;
use store::Store;
use tokio::sync::Semaphore;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub client: reqwest::Client,
    pub config: Arc<Config>,
    pub rate: Arc<RateLimiter>,
    pub preview_rate: Arc<RateLimiter>,
    pub semaphore: Arc<Semaphore>,
    pub preview_cache: Arc<handlers::PreviewCache>,
}

pub fn build_state(config: Config, store: Arc<Store>) -> AppState {
    AppState {
        store,
        client: build_client(),
        semaphore: Arc::new(Semaphore::new(config.max_upstream_concurrency)),
        rate: Arc::new(RateLimiter::per_minute(config.rate_limit_per_minute)),
        preview_rate: Arc::new(RateLimiter::per_minute(config.preview_limit_per_minute)),
        preview_cache: Arc::new(handlers::PreviewCache::new()),
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
        .route("/preview", get(handlers::preview))
        .route(
            "/avatar/:id",
            get(handlers::get_avatar).delete(handlers::delete_avatar),
        )
        .route("/download/:id", get(handlers::download))
        .route("/download-all", get(handlers::download_all))
        .route("/clear-all", post(handlers::clear_all))
        .route("/static/*path", get(handlers::static_file))
        .layer(SecurityHeadersLayer)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(60),
        ))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::store::Store;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn test_config(dir: &TempDir) -> Config {
        Config {
            port: 0,
            data_dir: dir.path().to_path_buf(),
            static_dir: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static"),
            robohash_base: "https://robohash.org".to_string(),
            default_ttl_days: 1,
            cleanup_interval_secs: 600,
            max_upstream_concurrency: 10,
            rate_limit_per_minute: 100,
            preview_limit_per_minute: 240,
            trust_forwarded_headers: false,
        }
    }

    #[test]
    fn build_state_creates_expected_state() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let store = Arc::new(Store::new(dir.path().to_path_buf()).unwrap());
        let state = build_state(config.clone(), store.clone());

        assert_eq!(state.config.port, 0);
        assert_eq!(state.config.max_upstream_concurrency, 10);
        assert_eq!(state.config.rate_limit_per_minute, 100);
        assert!(Arc::ptr_eq(&state.store, &store));
        assert_eq!(state.semaphore.available_permits(), 10);
        assert_eq!(state.config.preview_limit_per_minute, 240);
    }

    #[test]
    fn app_builds_router_with_all_routes() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let store = Arc::new(Store::new(dir.path().to_path_buf()).unwrap());
        let state = build_state(config, store);
        let router = app(state);

        // Router builds successfully - if it didn't, this would panic
        assert!(!format!("{:?}", router).is_empty());
    }

    #[tokio::test]
    async fn build_client_creates_valid_client() {
        let client = build_client();
        // Client should have the expected timeout and user agent
        // We can't easily inspect these, but we verify it builds
        assert!(!format!("{:?}", client).is_empty());
    }
}
