use std::{path::Path as FilePath, sync::Arc, time::UNIX_EPOCH};

use axum::{
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::typed_header::TypedHeaderRejection;
use htmlize::escape_text;
use indoc::formatdoc;
use mime_guess::mime;
use tokio::{
    fs::{self, File as TokioFile},
    io::AsyncWriteExt,
};
use tokio_util::io::ReaderStream;
use tracing::info;

use super::AppState;
use super::base_url::BaseUrl;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("File not found")]
    NotFound,
    #[error("Permission denied")]
    Forbidden,
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
            AppError::IoError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
            AppError::SystemTimeError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        (status, error_message).into_response()
    }
}

fn parse_filehash(file_hash: &str) -> (String, Option<String>) {
    let file_hash = FilePath::new(file_hash);
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

async fn file_to_timestamp(file: &TokioFile) -> Result<String, AppError> {
    Ok(file
        .metadata()
        .await?
        .modified()?
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .to_string())
}

/// Renders `content` as a syntax-highlighted HTML page using highlight.js.
fn highlighted_page(theme: &str, language: &str, content: &str) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static(mime::TEXT_HTML_UTF_8.as_ref()),
        )],
        formatdoc! {
            r#"
            <head>
                <link rel="stylesheet" href="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/styles/{}.css">
                <script src="https://cdn.jsdelivr.net/gh/highlightjs/cdn-release@11.9.0/build/highlight.min.js"></script>
                <script>hljs.highlightAll();</script>
                <style>
                    body {{
                        background-color: transparent;
                        transition: background-color 0.3s, color 0.3s;
                    }}
                    html {{
                        background-color: #f0f0f0;
                    }}
                </style>
            </head>
            <body>
            <pre><code class="{}">{}</code></pre>
            <script>
                function syncTheme() {{
                    const codeBlock = document.querySelector('.hljs');
                    
                    if (!codeBlock) return;

                    const style = window.getComputedStyle(codeBlock);
                    const bgColor = style.backgroundColor;
                    const textColor = style.color;

                    document.body.style.backgroundColor = bgColor;
                    document.body.style.color = textColor;
                    document.documentElement.style.backgroundColor = bgColor;
                }}

                window.addEventListener('load', syncTheme);
            </script>
            </body>
            "#,
            theme,
            language,
            escape_text(content)
        },
    )
        .into_response()
}

/// The README doubles as the index page. Its API examples use this origin,
/// which is replaced with the public base URL when serving the page.
const INDEX_PAGE: &str = include_str!("../../README.md");
const INDEX_EXAMPLE_ORIGIN: &str = "http://localhost:3000";

/// Serves the index page: syntax-highlighted Markdown for clients that accept
/// HTML (browsers), plain text otherwise (e.g. curl).
pub async fn index_handler(
    base_url: Result<BaseUrl, TypedHeaderRejection>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    let content = match base_url {
        Ok(BaseUrl(base_url)) => INDEX_PAGE.replace(INDEX_EXAMPLE_ORIGIN, &base_url),
        Err(_) => INDEX_PAGE.to_string(),
    };

    let accepts_html = headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| value.contains(mime::TEXT_HTML.as_ref()));

    let response = if accepts_html {
        highlighted_page(&state.args.syntax_theme, "md", &content)
    } else {
        content.into_response()
    };

    ([(header::VARY, header::ACCEPT.as_str())], response).into_response()
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
                    Ok(highlighted_page(
                        &state.args.syntax_theme,
                        &file_ext.unwrap_or_default(),
                        &content,
                    ))
                }
            }
            _ => match TokioFile::open(&file_path).await {
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

const HASHER: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
pub async fn put_handler(
    BaseUrl(base_url): BaseUrl,
    State(state): State<Arc<AppState>>,
    bytes: Bytes,
) -> Result<String, AppError> {
    use base64::prelude::*;

    let hash = &BASE64_URL_SAFE.encode(HASHER.checksum(&bytes).to_be_bytes())[0..4];

    let file_name = format!("{}.txt", hash);
    let file_path = FilePath::new(&state.args.file_path).join(file_name);
    let mut file = TokioFile::create(&file_path).await?;

    file.write_all(&bytes).await?;
    file.flush().await?;

    info!("File saved: hash: {} size: {} bytes", hash, bytes.len());

    let timestamp = file_to_timestamp(&file).await?;
    let secret = HASHER.checksum(timestamp.as_bytes()).to_string();

    Ok(formatdoc! {"
        url: {base_url}/{hash}
        short: {hash}
        size: {size} bytes
        secret: {secret}
        ",
        size = bytes.len(),
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

    let file = TokioFile::open(&file_path).await?;

    let timestamp = file_to_timestamp(&file).await?;
    let file_secret = HASHER.checksum(timestamp.as_bytes()).to_string();

    if file_path.exists() {
        if secret == file_secret {
            fs::remove_file(file_path).await?;
            Ok(format!("File {} deleted successfully", file_hash))
        } else {
            Err(AppError::Forbidden)
        }
    } else {
        Err(AppError::NotFound)
    }
}
