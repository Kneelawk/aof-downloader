use crate::download::DownloaderState;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn modpack_select(handle: AppHandle, downloader: State<Arc<DownloaderState>>, value: &str) {
    let modpack_path = PathBuf::from(value);

    let downloader = downloader.inner().clone();

    tauri::async_runtime::spawn(async move {
        downloader.start_download(modpack_path, handle).await;
    });
}

#[tauri::command]
pub async fn path_is_valid(value: &str) -> Result<bool, ()> {
    let path = PathBuf::from(value);
    Ok(match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata.is_file(),
        Err(_) => false,
    })
}
