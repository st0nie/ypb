use std::fs::File;
use std::io::Write;
use std::path::Path as FilePath;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::uri::Scheme;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use axum::response::{IntoResponse, Redirect};
use axum_extra::TypedHeader;
use axum_extra::headers::Host;
use indoc::formatdoc;
use thiserror::Error;
use tokio::fs::{self, File as TokioFile};
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
    IoError(#[from] tokio::io::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::IoError(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("IO Error: {}", err)),
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

fn file_to_timestamp(file: &File) -> Result<String, AppError> {
    file.metadata()?
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::InternalServerError(format!("System time error: {}", e)))?
        .as_secs()
        .to_string()
        .pipe(Ok)
}

pub async fn get_handler(
    Path(file_hash): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, AppError> {
    let (file_name, file_ext) = parse_filehash(file_hash.as_str());

    let dir = &state.args.file_path;
    let file_path = FilePath::new(dir).join(file_name);

    if file_path.exists() {
        match fs::read_to_string(&file_path).await {
            Ok(content) =>
            // 302 redirect if the content is a valid URL
            {
                if content.starts_with("http") && !content.contains([' ', '\n']) {
                    Ok(Redirect::temporary(&content).into_response())
                } else if file_ext.as_ref().is_none_or(|ext| ext == "txt") {
                    Ok(content.into_response())
                } else {
                    Ok((
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
                            <pre><code class="{}">{}</code></pre>
                            </body>
                            "#,
                            state.args.syntax_theme,
                            file_ext.unwrap_or_default(),
                            content
                        }
                    ).into_response())
                }
            }
            Err(_) => match TokioFile::open(&file_path).await {
                Ok(file) => {
                    let stream = ReaderStream::new(file);
                    let body = Body::from_stream(stream);
                    let content_type = mime_guess::from_ext(&file_ext.unwrap_or_default())
                        .first_or_octet_stream()
                        .to_string();

                    Ok(
                        (StatusCode::OK, [(header::CONTENT_TYPE, content_type)], body)
                            .into_response(),
                    )
                }
                Err(e) => Err(AppError::IoError(e)),
            },
        }
    } else {
        Err(AppError::NotFound)
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

    let hash = &BASE64_URL_SAFE.encode(HASHER.checksum(&bytes).to_be_bytes())[0..4];

    let file_name = format!("{}.txt", hash);
    let file_path = FilePath::new(&state.args.file_path).join(file_name);
    let mut file = File::create(&file_path)?;

    file.write_all(&bytes)?;

    info!("File saved: hash: {} size: {} bytes", hash, bytes.len());

    let protocal_str = header_map
        .get("X-Forwarded-Proto")
        .and_then(|proto| proto.to_str().ok())
        .unwrap_or(Scheme::HTTP.as_str());

    let timestamp = file_to_timestamp(&file)?;

    Ok(formatdoc! {"
        url: {protocal}://{host}/{hash}
        short: {hash}
        size: {size} bytes
        secret: {timestamp}
        ",
        protocal = protocal_str,
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
    let file_path = FilePath::new(dir).join(file_name);

    let file = File::open(&file_path)?;

    let timestamp = file_to_timestamp(&file)?;

    if file_path.exists() {
        if secret == timestamp {
            fs::remove_file(file_path)
                .await?;
            Ok(format!("File {} deleted successfully", file_hash))
        } else {
            Err(AppError::Forbidden)
        }
    } else {
        Err(AppError::NotFound)
    }
}

// Helper function to pipe a value into Ok, useful for chaining
trait Pipeable {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T where Self: Sized {
        f(self)
    }
}

impl<T> Pipeable for T {}
