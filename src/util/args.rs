use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    pub port: u16,

    /// Path to the file storage directory
    #[clap(short, long, default_value = "./files")]
    pub storage_path: String,

    /// Period to check for expired files (in seconds)
    #[clap(short, long, default_value_t = 3600)]
    pub expired_check_period: u64,
}
