use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::DefaultBodyLimit;
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
use util::handler::{delete_handler, get_handler, index_handler, put_handler};
use util::{AppState, Args};

const DEFAULT_LOG_LEVEL: LevelFilter = if cfg!(debug_assertions) {
    LevelFilter::DEBUG
} else {
    LevelFilter::INFO
};

fn app(app_state: Arc<AppState>) -> Router {
    let limit_size = app_state.args.limit_size;
    Router::new()
        .route("/", get(index_handler).put(put_handler))
        .route("/{*hash}", get(get_handler).delete(delete_handler))
        .layer((
            TraceLayer::new_for_http(),
            // Graceful shutdown will wait for outstanding requests to complete. Add a timeout so
            // requests don't hang forever.
            TimeoutLayer::new(Duration::from_secs(10)),
            DefaultBodyLimit::max(limit_size),
        ))
        .with_state(app_state)
}

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

    let app = app(Arc::new(AppState {
        args: Arc::new(args.clone()),
    }));

    // Create a `TcpListener` using tokio.
    let listener = TcpListener::bind(format!("0.0.0.0:{}", args.port))
        .await
        .context(format!("Failed to listen on port {}", args.port))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use axum::response::Response;
    use tower::ServiceExt;

    const BROWSER_ACCEPT: &str = "text/html,application/xhtml+xml,*/*;q=0.8";

    struct TestApp {
        router: Router,
        dir: std::path::PathBuf,
    }

    impl TestApp {
        fn new(name: &str, extra_args: &[&str]) -> Self {
            let dir = std::env::temp_dir().join(format!("ypb-test-{}-{name}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let args = Args::parse_from(
                ["ypb", "--file-path", dir.to_str().unwrap()]
                    .iter()
                    .chain(extra_args),
            );
            let router = app(Arc::new(AppState {
                args: Arc::new(args),
            }));
            Self { router, dir }
        }

        async fn send(&self, request: Request<Body>) -> Response {
            self.router.clone().oneshot(request).await.unwrap()
        }
    }

    impl Drop for TestApp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    async fn body_text(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn content_type(response: &Response) -> &str {
        response.headers()[header::CONTENT_TYPE].to_str().unwrap()
    }

    #[tokio::test]
    async fn index_is_plain_text_with_request_base_url() {
        let app = TestApp::new("index-plain", &[]);
        let response = app
            .send(
                Request::get("/")
                    .header(header::HOST, "pb.s-ton.top:4443")
                    .header("X-Forwarded-Proto", "https")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "text/plain; charset=utf-8");
        assert_eq!(response.headers()[header::VARY], "accept");
        let body = body_text(response).await;
        assert!(body.starts_with("# YPB"));
        assert!(body.contains(r#"curl -X PUT --data-binary @- "https://pb.s-ton.top:4443/""#));
        assert!(body.contains("$ curl https://pb.s-ton.top:4443/K8mw"));
        assert!(!body.contains("localhost:3000"));
    }

    #[tokio::test]
    async fn index_is_highlighted_html_for_browsers() {
        let app = TestApp::new("index-html", &["--base-url", "https://pb.example.com/"]);
        let response = app
            .send(
                Request::get("/")
                    .header(header::HOST, "internal:3000")
                    .header(header::ACCEPT, BROWSER_ACCEPT)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "text/html; charset=utf-8");
        assert_eq!(response.headers()[header::VARY], "accept");
        let body = body_text(response).await;
        assert!(body.contains(r#"<pre><code class="md"># YPB"#));
        assert!(body.contains("styles/vs.css"));
        assert!(body.contains("$ curl https://pb.example.com/K8mw"));
        assert!(!body.contains("localhost:3000"));
        assert!(!body.contains("internal:3000"));
    }

    #[tokio::test]
    async fn index_without_host_keeps_example_origin() {
        let app = TestApp::new("index-no-host", &[]);
        let response = app
            .send(Request::get("/").body(Body::empty()).unwrap())
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            body_text(response)
                .await
                .contains("$ curl http://localhost:3000/K8mw")
        );
    }

    #[tokio::test]
    async fn upload_retrieve_and_delete_round_trip() {
        let app = TestApp::new("round-trip", &[]);
        let response = app
            .send(
                Request::put("/")
                    .header(header::HOST, "pb.s-ton.top:4443")
                    .header("X-Forwarded-Proto", "https")
                    .body(Body::from("# hello\n"))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 4);
        let short = lines[1].strip_prefix("short: ").unwrap();
        assert_eq!(lines[0], format!("url: https://pb.s-ton.top:4443/{short}"));
        assert_eq!(lines[2], "size: 8 bytes");
        let secret = lines[3].strip_prefix("secret: ").unwrap().to_string();

        let response = app
            .send(
                Request::get(format!("/{short}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(content_type(&response), "text/plain; charset=utf-8");
        assert_eq!(body_text(response).await, "# hello\n");

        let response = app
            .send(
                Request::get(format!("/{short}.md"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(content_type(&response), "text/html; charset=utf-8");
        assert!(
            body_text(response)
                .await
                .contains(r#"<pre><code class="md"># hello"#)
        );

        let response = app
            .send(
                Request::delete(format!("/{short}"))
                    .body(Body::from("0"))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .send(
                Request::delete(format!("/{short}"))
                    .body(Body::from(secret))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            body_text(response).await,
            format!("File {short} deleted successfully")
        );

        let response = app
            .send(
                Request::get(format!("/{short}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn upload_url_uses_configured_base_url() {
        let app = TestApp::new("put-base-url", &["--base-url", "https://pb.example.com"]);
        let response = app
            .send(Request::put("/").body(Body::from("data")).unwrap())
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            body_text(response)
                .await
                .starts_with("url: https://pb.example.com/")
        );
    }

    #[tokio::test]
    async fn upload_without_host_or_base_url_is_rejected() {
        let app = TestApp::new("put-no-host", &[]);
        let response = app
            .send(Request::put("/").body(Body::from("data")).unwrap())
            .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
