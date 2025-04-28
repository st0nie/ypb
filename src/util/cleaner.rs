use anyhow::Result;
use std::fs;
use std::path::Path;
use tracing::{error, info};

pub async fn cleaner_task(storage_path: String, period: u64) -> Result<()> {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(period)).await;
        info!("Cleaning up expired files in {:?}", storage_path);
        if let Err(e) = clean_up(&storage_path, period) {
            error!("Error cleaning up files: {:?}", e);
        }
    }
}

fn clean_up(dir: &str, period: u64) -> Result<()> {
    let path = Path::new(dir);

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_path = entry.path();

        let metadata = fs::metadata(&file_path)?;

        let last_modified = metadata.modified()?.elapsed()?.as_secs();

        if last_modified > period {
            fs::remove_file(&file_path)?;
            info!("Deleted file: {:?}", file_path);
        }
    }
    Ok(())
}
