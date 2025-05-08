use anyhow::Result;
use std::path::Path;
use tokio::{fs, task::JoinSet};
use tracing::{error, info};

pub async fn cleaner_task(storage_path: String, period: u64) -> Result<()> {
    tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
    loop {
        info!("Cleaning up expired files in {:?}", storage_path);
        if let Err(e) = clean_up(&storage_path, period).await {
            error!("Error cleaning up files: {:?}", e);
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(period)).await;
    }
}

async fn clean_up(dir: &str, period: u64) -> Result<()> {
    let path = Path::new(dir);
    let mut read_dir = fs::read_dir(path).await?;

    let mut set = JoinSet::new();

    while let Some(entry) = read_dir.next_entry().await? {
        set.spawn(async move {
            let file_path = entry.path();
            let metadata = fs::metadata(&file_path).await?;
            let last_modified = metadata.modified()?.elapsed()?.as_secs();

            if last_modified > period {
                fs::remove_file(&file_path).await?;
                info!("Deleted file: {:?}", file_path);
            }
            anyhow::Ok(())
        });
    }

    set.join_all().await.into_iter().for_each(|result| {
        if let Err(e) = result {
            error!("Error deleting file: {:?}", e);
        }
    });
    Ok(())
}
