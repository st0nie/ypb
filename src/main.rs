use std::sync::Arc;
use std::time::Duration;

use axum::routing::put;
use axum::{Router, routing::get};
use clap::Parser;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub mod util;
use util::handler::{get_handler, put_handler};
use util::{AppState, Args};

#[tokio::main]
async fn main() {
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

    if !std::path::Path::new(&args.storage_path).exists() {
        std::fs::create_dir_all(&args.storage_path).expect("Failed to create storage directory");
    }

    let app_state = Arc::new(AppState {
        storage_path: args.storage_path.clone(),
    });
    // Create a regular axum app.
    let app = Router::new()
        .route("/", get("hello, ypb!"))
        .route("/", put(put_handler))
        .route("/{*hash}", get(get_handler))
        .layer((
            TraceLayer::new_for_http(),
            // Graceful shutdown will wait for outstanding requests to complete. Add a timeout so
            // requests don't hang forever.
            TimeoutLayer::new(Duration::from_secs(10)),
        ))
        .with_state(app_state);

    // Create a `TcpListener` using tokio.
    let listener = TcpListener::bind(format!("0.0.0.0:{}", args.port))
        .await
        .unwrap();

    tokio::spawn(util::cleaner::cleaner_task(
        args.storage_path,
        args.expired_check_period,
    ));
    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
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
