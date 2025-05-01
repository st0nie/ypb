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
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod util;
use util::handler::{delete_handler, get_handler, put_handler};
use util::{AppState, Args};

// Function to create the application router
pub fn create_app_router(app_state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(|| async { "hello, ypb!" }))
        .route("/", put(put_handler))
        .route("/{*hash}", get(get_handler))
        .route("/{*hash}", delete(delete_handler))
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::new(Duration::from_secs(10)),
            DefaultBodyLimit::max(app_state.args.limit_size),
        ))
        .with_state(app_state)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Enable tracing.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!(
                    "{}=debug,tower_http=debug,axum=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer().without_time())
        .init();

    let args = Args::parse();

    // Ensure storage directory exists
    let storage_path = std::path::Path::new(&args.file_path);
    if !storage_path.exists() {
        std::fs::create_dir_all(storage_path)
            .with_context(|| format!("Failed to create storage directory: {}", args.file_path))?;
    }

    let app_state = Arc::new(AppState { args: args.clone() });

    // Create the app router using the dedicated function
    let app = create_app_router(app_state);

    // Create a `TcpListener` using tokio.
    let listener_addr = format!("0.0.0.0:{}", args.port);
    let listener = TcpListener::bind(&listener_addr)
        .await
        .with_context(|| format!("Failed to bind to address {}", listener_addr))?;
    tracing::debug!("listening on {}", listener.local_addr()?);

    // Spawn the cleaner task
    tokio::spawn(util::cleaner::cleaner_task(
        args.file_path.clone(), // Clone path for the task
        args.clean_period,
    ));

    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

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

#[cfg(test)]
mod tests;
