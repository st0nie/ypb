use anyhow::Result;
use std::path::Path;
use tokio::fs;
use tracing::{error, info};

pub async fn cleaner_task(storage_path: String, period: u64) -> Result<()> {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(period)).await;
        info!("Cleaning up expired files in {:?}", storage_path);
        if let Err(e) = clean_up(&storage_path, period).await {
            error!("Error cleaning up files: {:?}", e);
        }
    }
}

async fn clean_up(dir: &str, period: u64) -> Result<()> {
    let path = Path::new(dir);
    let mut read_dir = fs::read_dir(path).await?;

    while let Some(entry) = read_dir.next_entry().await? {
        let file_path = entry.path();
        let metadata = fs::metadata(&file_path).await?;
        let last_modified = metadata.modified()?.elapsed()?.as_secs();

        if last_modified > period {
            fs::remove_file(&file_path).await?;
            info!("Deleted file: {:?}", file_path);
        }
    }
    Ok(())
}
