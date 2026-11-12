use avatar_generator::{app, build_state, config::Config, handlers, store::Store};
use chrono::Utc;
use std::{sync::Arc, time::Duration};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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

#[tokio::main]
async fn main() {
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
    let warm_state = state.clone();
    tokio::spawn(async move { handlers::warm_previews(warm_state).await });
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port))
        .await
        .expect("failed to bind port");
    tracing::info!(port = config.port, "listening");
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .expect("server error");
}
