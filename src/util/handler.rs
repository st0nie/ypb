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
use tokio::fs::{self, File as TokioFile};
use tokio_util::io::ReaderStream;
use tracing::info;

use super::AppState;

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

fn file_to_timestamp(file: &File) -> Result<String, StatusCode> {
    Ok(file
        .metadata()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .modified()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .as_secs()
        .to_string())
}

pub async fn get_handler(
    Path(file_hash): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, StatusCode> {
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
                } else if file_ext.is_none_or(|ext| ext == "txt") {
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
                            <pre><code>{}</code></pre>
                            </body>
                            "#,
                            state.args.syntax_theme,
                            content
                        }
                    ).into_response())
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
                _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
            },
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn put_handler(
    TypedHeader(host): TypedHeader<Host>,
    header_map: HeaderMap,
    State(state): State<Arc<AppState>>,
    bytes: Bytes,
) -> Result<String, StatusCode> {
    if bytes.len() > state.args.limit_size {
        return Err(StatusCode::BAD_REQUEST);
    }

    const HASHER: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

    use base64::prelude::*;

    let hash = &BASE64_URL_SAFE.encode(HASHER.checksum(&bytes).to_be_bytes())[0..4];

    let file_name = format!("{}.txt", hash);
    let file_path = FilePath::new(&state.args.file_path).join(file_name);
    let mut file = File::create(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    file.write_all(&bytes)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
) -> Result<String, StatusCode> {
    let (file_name, _) = parse_filehash(file_hash.as_str());

    let dir = &state.args.file_path;
    let file_path = FilePath::new(dir).join(file_name);

    let file = File::open(&file_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let timestamp = file_to_timestamp(&file)?;

    if file_path.exists() {
        if secret == timestamp {
            fs::remove_file(file_path)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            Ok(format!("File {} deleted successfully", file_hash))
        } else {
            Err(StatusCode::FORBIDDEN)
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}