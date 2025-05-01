use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::uri::Scheme;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use axum::response::{IntoResponse, Redirect};
use axum_extra::TypedHeader;
use axum_extra::headers::Host;
use futures::TryFutureExt;
use indoc::formatdoc;
use std::path::Path as FilePath;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use thiserror::Error;
use tokio::fs::{self, File as TokioFile};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use tracing::info;

use super::AppState;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("File not found")]
    NotFound,
    #[error("Permission denied")]
    Forbidden,
    #[error("Internal server error: {0}")]
    InternalServerError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("System time error: {0}")]
    SystemTimeError(#[from] std::time::SystemTimeError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::IoError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            AppError::SystemTimeError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        (status, error_message).into_response()
    }
}

fn parse_filehash(file_hash: &str) -> (String, Option<String>) {
    let file_hash = std::path::Path::new(file_hash);
    let file_name = format!(
        "{}.txt",
        file_hash
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
    );
    let file_ext = file_hash
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    (file_name, file_ext)
}

async fn file_to_timestamp(file_path: &FilePath) -> Result<String, AppError> {
    Ok(fs::metadata(file_path)
        .await?
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .to_string())
}

pub async fn get_handler(
    Path(file_hash): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let (file_name, file_ext) = parse_filehash(file_hash.as_str());

    let dir = &state.args.file_path;
    let file_path = FilePath::new(dir).join(&file_name);

    if !file_path.exists() {
        return Err(AppError::NotFound);
    }

    match file_ext.as_deref() {
        None | Some("txt") => {
            let content = fs::read_to_string(&file_path).await?;
            if content.starts_with("http") && !content.contains([' ', '\n']) {
                Ok(Redirect::temporary(&content).into_response())
            } else {
                Ok(content.into_response())
            }
        }
        Some(ext) => {
            // Attempt to read as string and format as HTML
            let attempt_html = async {
                let context = fs::read_to_string(&file_path).await?;
                Ok::<_, AppError>( // Success type is Response
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                        formatdoc! {
                            r#"
                            <head>
                                <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/styles/{}.css">
                                <script src="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/highlight.min.js"></script>
                                <script>hljs.highlightAll();</script>
                            </head>
                            <body>
                            <pre><code class="{ext}">{context}</code></pre>
                            </body>
                            "#,
                            state.args.syntax_theme,
                        }
                    ).into_response()
                )
            };

            // Fallback: stream the file directly if reading as string fails
            let fallback_stream = |_| async {
                let body = TokioFile::open(&file_path)
                    .map_ok(ReaderStream::new)
                    .map_ok(Body::from_stream)
                    .await?;
                let content_type = mime_guess::from_ext(ext)
                    .first_or_octet_stream()
                    .to_string();

                Ok::<_, AppError>(
                    (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body).into_response(),
                )
            };

            attempt_html.or_else(fallback_stream).await
        }
    }
}

pub async fn put_handler(
    TypedHeader(host): TypedHeader<Host>,
    header_map: HeaderMap,
    State(state): State<Arc<AppState>>,
    bytes: Bytes,
) -> Result<String, AppError> {
    const HASHER: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

    use base64::prelude::*;

    let hash_bytes = HASHER.checksum(&bytes).to_be_bytes();
    let hash = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&hash_bytes[0..4]);

    let file_name = format!("{}.txt", hash);
    let file_path = FilePath::new(&state.args.file_path).join(&file_name);

    let mut file = TokioFile::create(&file_path).await?;
    file.write_all(&bytes).await?;
    file.sync_all().await?;

    info!(
        "File saved: path: {:?} size: {} bytes",
        file_path,
        bytes.len()
    );

    let protocol_str = header_map
        .get("X-Forwarded-Proto")
        .and_then(|proto| proto.to_str().ok())
        .unwrap_or_else(|| Scheme::HTTP.as_str());

    let timestamp = file_to_timestamp(&file_path).await?;

    Ok(formatdoc! {"
        url: {protocol}://{host}/{hash}
        short: {hash}
        size: {size} bytes
        secret: {timestamp}
        ",
        protocol = protocol_str,
        size = bytes.len(),
        hash = hash,
        host = host,
        timestamp = timestamp
    })
}

pub async fn delete_handler(
    Path(file_hash): Path<String>,
    State(state): State<Arc<AppState>>,
    secret: String,
) -> Result<String, AppError> {
    let (file_name, _) = parse_filehash(file_hash.as_str());

    let dir = &state.args.file_path;
    let file_path = FilePath::new(dir).join(&file_name);

    if !file_path.exists() {
        return Err(AppError::NotFound);
    }

    let timestamp = file_to_timestamp(&file_path).await?;

    if secret == timestamp {
        fs::remove_file(&file_path).await?;
        Ok(format!("File {} deleted successfully", file_hash))
    } else {
        Err(AppError::Forbidden)
    }
}
