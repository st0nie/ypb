use std::sync::Arc;

use axum::{
    RequestPartsExt,
    extract::FromRequestParts,
    http::{request::Parts, uri::Scheme},
};
use axum_extra::{TypedHeader, headers::Host, typed_header::TypedHeaderRejection};

use super::AppState;

/// Public base URL of the service, without a trailing slash.
///
/// Uses `--base-url` when configured; otherwise it is built from the
/// `X-Forwarded-Proto` header (defaulting to `http`) and the `Host` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl(pub String);

impl FromRequestParts<Arc<AppState>> for BaseUrl {
    type Rejection = TypedHeaderRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        if let Some(base_url) = &state.args.base_url {
            return Ok(BaseUrl(base_url.clone()));
        }

        let TypedHeader(host) = parts.extract::<TypedHeader<Host>>().await?;

        let scheme = parts
            .headers
            .get("X-Forwarded-Proto")
            .and_then(|proto| proto.to_str().ok())
            .unwrap_or(Scheme::HTTP.as_str());

        Ok(BaseUrl(format!("{scheme}://{host}")))
    }
}
