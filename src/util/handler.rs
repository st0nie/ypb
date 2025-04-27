use std::fs::{self, File};
use std::io::Write;
use std::path::Path as FilePath;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum_extra::TypedHeader;
use axum_extra::headers::Host;
use indoc::formatdoc;

use super::AppState;

pub async fn get_handler(
    Path(file_hash): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Result<String, StatusCode> {
    let file_name = format!("{}.txt", file_hash);
    let dir = &state.storage_path;
    let file_path = FilePath::new(dir).join(file_name);

    if file_path.exists() {
        match fs::read_to_string(file_path) {
            Ok(content) => Ok(content),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

pub async fn put_handler(
    TypedHeader(host): TypedHeader<Host>,
    State(state): State<Arc<AppState>>,
    bytes: Bytes,
) -> Result<String, StatusCode> {
    if bytes.len() > state.size_limit {
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

    Ok(formatdoc! {"
        url: http://{host}/{hash}
        size: {size} bytes
        ",
        size = bytes.len(),
        hash = hash,
        host = host,
    })
}
