use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::DefaultBodyLimit;
use axum::routing::{delete, put};
use axum::{Router, routing::get};
use clap::Parser;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod util;
use util::handler::{delete_handler, get_handler, put_handler};
use util::{AppState, Args};

const DEFAULT_LOG_LEVEL: LevelFilter = if cfg!(debug_assertions) {
    LevelFilter::DEBUG
} else {
    LevelFilter::INFO
};

#[tokio::main]
async fn main() -> Result<()> {
    // Enable tracing.
    tracing_subscriber::registry()
        .with(
            EnvFilter::builder()
                .with_default_directive(DEFAULT_LOG_LEVEL.into())
                .from_env_lossy(),
        )
        .with(tracing_subscriber::fmt::layer().without_time())
        .init();

    let args = Args::parse();

    if !std::path::Path::new(&args.file_path).exists() {
        std::fs::create_dir_all(&args.file_path).context("Failed to create storage directory")?;
    }

    let app_state = Arc::new(AppState { args: args.clone() });
    // Create a regular axum app.
    let app = Router::new()
        .route("/", get("hello, ypb!"))
        .route("/", put(put_handler))
        .route("/{*hash}", get(get_handler))
        .route("/{*hash}", delete(delete_handler))
        .layer((
            TraceLayer::new_for_http(),
            // Graceful shutdown will wait for outstanding requests to complete. Add a timeout so
            // requests don't hang forever.
            TimeoutLayer::new(Duration::from_secs(10)),
            DefaultBodyLimit::max(args.limit_size),
        ))
        .with_state(app_state);

    // Create a `TcpListener` using tokio.
    let listener = TcpListener::bind(format!("0.0.0.0:{}", args.port))
        .await
        .with_context(|| format!("Failed to listen on port {}", args.port))?;

    tokio::spawn(util::cleaner::cleaner_task(
        args.file_path,
        args.clean_period,
    ));
    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
