use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    pub port: u16,

    /// Path to the file storage directory
    #[clap(short, long, default_value = "./files")]
    pub file_path: String,

    /// Period to check for expired files (in seconds)
    #[clap(short, long, default_value_t = 3600)]
    pub clean_period: u64,

    /// File size limit (in bytes)
    #[clap(short, long, default_value_t = 10 * 1024 * 1024)]
    pub limit_size: usize,

    /// Syntax highlight theme (highlight.js)
    #[clap(short, long, default_value = "vs")]
    pub syntax_theme: String,

    /// Path to the web static files directory
    #[clap(short, long, default_value = "./web")]
    pub web_path: String,
}
