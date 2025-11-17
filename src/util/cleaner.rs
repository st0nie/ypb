use anyhow::Result;
use futures::StreamExt;
use std::path::Path;
use tokio::fs;
use tracing::{debug, error, info};

const CLEAN_CHECK_PERIOD_SECS: u64 = 60; // 1 minute

pub async fn cleaner_task(storage_path: String, period: u64) -> Result<()> {
    tokio::time::sleep(tokio::time::Duration::from_secs(CLEAN_CHECK_PERIOD_SECS)).await;
    loop {
        debug!("Cleaning up expired files in {:?}", storage_path);
        if let Err(e) = clean_up(&storage_path, period).await {
            error!("Error cleaning up files: {:?}", e);
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(CLEAN_CHECK_PERIOD_SECS)).await;
    }
}

async fn clean_up(dir: &str, period: u64) -> Result<()> {
    let path = Path::new(dir);
    let read_dir = fs::read_dir(path).await?;

    let read_dir_stream = tokio_stream::wrappers::ReadDirStream::new(read_dir);

    read_dir_stream
        .for_each_concurrent(10, |entry| async move {
            let result: Result<()> = async {
                let entry = entry?;
                let file_path = entry.path();
                let metadata = fs::metadata(&file_path).await?;
                let last_modified = metadata.modified()?.elapsed()?.as_secs();

                if last_modified > period {
                    fs::remove_file(&file_path).await?;
                    info!("Deleted file: {:?}", file_path);
                }
                anyhow::Ok(())
            }
            .await;

            if let Err(e) = result {
                error!("Error processing file: {:?}", e);
            }
        })
        .await;

    Ok(())
}
