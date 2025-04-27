use std::fs::File;
use std::io::Write;
use std::path::Path as FilePath;
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::uri::Scheme;
use axum::http::{StatusCode, Uri};
use axum::response::IntoResponse;
use axum::response::Response;
use axum_extra::TypedHeader;
use axum_extra::headers::Host;
use indoc::formatdoc;
use tokio::fs::{self, File as TokioFile};
use tokio_util::io::ReaderStream;
use tracing::info;

use super::AppState;

pub async fn get_handler(
    Path(file_hash): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Response, StatusCode> {
    let file_name = format!("{}.txt", file_hash);
    let dir = &state.storage_path;
    let file_path = FilePath::new(dir).join(file_name);

    if file_path.exists() {
        match fs::read_to_string(&file_path).await {
            Ok(content) => Ok(content.into_response()),
            _ => match TokioFile::open(&file_path).await {
                Ok(file) => {
                    let stream = ReaderStream::new(file);
                    let body = Body::from_stream(stream);
                    Ok(body.into_response())
                }
                _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
            },
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn put_handler(
    uri: Uri,
    TypedHeader(host): TypedHeader<Host>,
    State(state): State<Arc<AppState>>,
    bytes: Bytes,
) -> Result<String, StatusCode> {
    if bytes.len() > state.limit_size {
        return Err(StatusCode::BAD_REQUEST);
    }

    const HASHER: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);

    use base64::prelude::*;

    let hash = &BASE64_STANDARD.encode(HASHER.checksum(&bytes).to_be_bytes())[0..4];

    let file_name = format!("{}.txt", hash);
    let file_path = FilePath::new(&state.storage_path).join(file_name);
    let mut file = match File::create(&file_path) {
        Ok(file) => file,
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    if let Err(_) = file.write_all(&bytes) {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    info!("File saved: hash: {} size: {} bytes", hash, bytes.len());

    let protocal_str = uri.scheme_str().unwrap_or(Scheme::HTTP.as_str());

    Ok(formatdoc! {"
        url: {protocal}://{host}/{hash}
        size: {size} bytes
        ",
        protocal = protocal_str,
        size = bytes.len(),
        hash = hash,
        host = host,
    })
}
