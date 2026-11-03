use clap::Parser;

/// Yet another PasteBin.
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    pub port: u16,

    /// Path to the file storage directory
    #[arg(short, long, default_value = "./files")]
    pub file_path: String,

    /// Period to check for expired files (in seconds)
    #[arg(short, long, default_value_t = 3600)]
    pub clean_period: u64,

    /// File size limit (in bytes)
    #[arg(short, long, default_value_t = 10 * 1024 * 1024)]
    pub limit_size: usize,

    /// Syntax highlight theme (highlight.js)
    #[arg(short, long, default_value = "vs")]
    pub syntax_theme: String,

    /// Public base URL used in generated links and on the index page,
    /// e.g. `https://pb.example.com`. When unset, it is derived from the
    /// request's `X-Forwarded-Proto` and `Host` headers.
    #[arg(short, long, value_parser = parse_base_url)]
    pub base_url: Option<String>,
}

/// Validates a public base URL and normalizes it by stripping trailing slashes.
fn parse_base_url(value: &str) -> Result<String, String> {
    let uri: axum::http::Uri = value.parse().map_err(|err| format!("invalid URL: {err}"))?;

    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return Err("URL scheme must be http or https".into());
    }
    if uri.authority().is_none() {
        return Err("URL must contain a host".into());
    }
    if uri.query().is_some() {
        return Err("URL must not contain a query string".into());
    }

    Ok(value.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_base_url;

    #[test]
    fn base_url_is_normalized() {
        assert_eq!(
            parse_base_url("https://pb.s-ton.top:4443/").as_deref(),
            Ok("https://pb.s-ton.top:4443")
        );
        assert_eq!(
            parse_base_url("http://example.com/pb//").as_deref(),
            Ok("http://example.com/pb")
        );
    }

    #[test]
    fn invalid_base_url_is_rejected() {
        assert!(parse_base_url("pb.s-ton.top").is_err());
        assert!(parse_base_url("ftp://pb.s-ton.top").is_err());
        assert!(parse_base_url("https://pb.s-ton.top/?a=b").is_err());
        assert!(parse_base_url("not a url").is_err());
    }
}
