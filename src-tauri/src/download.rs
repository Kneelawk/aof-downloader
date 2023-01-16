use crate::json::ManifestJson;
use crate::utils::ResultExt;
use anyhow::Context;
use async_zip::read::fs::ZipFileReader;
use bytes::{Buf, Bytes};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use indicatif::HumanBytes;
use reqwest::{Client, Url};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio::{fs, io};

const MAX_CONCURRENT_DOWNLOADS: usize = 100;
const MAX_CONCURRENT_EXTRACTIONS: usize = 100;

#[tauri::command]
pub async fn get_status(
    downloader: State<'_, Arc<DownloaderState>>,
    url: &str,
) -> Result<FileStatus, String> {
    downloader
        .status_manager
        .get(url)
        .await
        .ok_or_else(|| format!("No state for {}", url))
}

#[tauri::command]
pub fn human_bytes(bytes: u64) -> String {
    format!("{}", HumanBytes(bytes))
}

pub struct DownloaderState {
    downloading: AtomicBool,
    client: Arc<Client>,
    status_manager: Arc<FileStatusManager>,
}

impl DownloaderState {
    pub fn new() -> Self {
        Self {
            downloading: AtomicBool::new(false),
            client: Arc::new(Client::new()),
            status_manager: Arc::new(FileStatusManager::new()),
        }
    }

    pub async fn start_download(&self, modpack_path: impl AsRef<Path>, handle: AppHandle) {
        if !self.downloading.swap(true, Ordering::AcqRel) {
            handle.emit_all("download_start", ()).unwrap();

            match self
                .do_download(modpack_path.as_ref(), handle.clone())
                .await
            {
                Ok(_) => {
                    handle.emit_all("download_complete", ()).unwrap();
                    self.downloading.store(false, Ordering::Release);
                }
                Err(e) => {
                    handle.emit_all("download_error", e.to_string()).unwrap();
                    eprintln!("{:?}", e);
                }
            }
        }
    }

    async fn do_download(&self, modpack_path: &Path, handle: AppHandle) -> anyhow::Result<()> {
        let output_path = modpack_path
            .parent()
            .ok_or(anyhow!("Modpack file has no parent dir"))?
            .join(
                modpack_path
                    .file_stem()
                    .ok_or(anyhow!("Modpack file is missing .zip extension"))?,
            );

        println!("Extracting modpack to: {}", output_path.to_string_lossy());

        let mods_path = output_path.join("mods");

        if !fs::metadata(&mods_path).await.is_ok() {
            fs::create_dir_all(&mods_path)
                .await
                .context("Error creating downloaded 'mods' dir")?;
        }

        let modpack_zip = Arc::new(
            ZipFileReader::new(modpack_path)
                .await
                .context("Error opening modpack zip")?,
        );

        // Extract overrides
        println!("Extracting overrides...");
        extract_overrides(handle.clone(), output_path, modpack_zip.clone())
            .await
            .context("Error extracting overrides")?;

        // Read manifest
        println!("Reading manifest...");
        let (manifest_index, _manifest_entry) = modpack_zip
            .entry("manifest.json")
            .context("Error finding manifest")?;
        let manifest_reader = modpack_zip
            .entry_reader(manifest_index)
            .await
            .context("Error opening manifest")?;
        let manifest_str = manifest_reader
            .read_to_string_crc()
            .await
            .context("Error reading manifest")?;
        let manifest: ManifestJson =
            serde_json::from_str(&manifest_str).context("Error parsing manifest")?;

        // Download mods in manifest
        println!("Downloading mods...");
        download_mods(
            self.client.clone(),
            handle,
            self.status_manager.clone(),
            manifest,
            mods_path,
        )
        .await
        .context("Error downloading mods")?;

        println!("Done.");

        Ok(())
    }
}

async fn extract_overrides(
    handle: AppHandle,
    output_path: PathBuf,
    modpack_zip: Arc<ZipFileReader>,
) -> anyhow::Result<()> {
    let entries = modpack_zip.entries();
    handle.emit_all("extraction_total", entries.len()).unwrap();

    let canceled = Arc::new(AtomicBool::new(false));
    let extraction_count = Arc::new(AtomicUsize::new(0));
    let mut futures: FuturesUnordered<JoinHandle<anyhow::Result<()>>> = FuturesUnordered::new();

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_EXTRACTIONS));

    for index in 0..entries.len() {
        let entry = entries[index];
        let entry_filename = entry.filename().replace('\\', "/");
        if !entry_filename.starts_with("overrides/") && !entry_filename.starts_with("/overrides/") {
            continue;
        }

        let modpack_zip = modpack_zip.clone();
        let handle = handle.clone();
        let output_path = output_path.clone();
        let extraction_count = extraction_count.clone();
        let canceled = canceled.clone();
        let semaphore = semaphore.clone();

        futures.push(tokio::spawn(async move {
            let _permit = semaphore
                .acquire()
                .await
                .context("Error acquiring semaphore permit")?;

            if canceled.load(Ordering::Acquire) {
                return Ok(());
            }

            let mut entry_reader = modpack_zip
                .entry_reader(index)
                .await
                .context("Error reading zip entry")
                .cancel(&canceled)?;
            let entry = entry_reader.entry();

            let entry_filename = entry.filename().replace('\\', "/");
            let extract_path = output_path.join(sanitize_override_path(&entry_filename));

            if !entry_filename.ends_with('/') {
                // Create parent dir if not already existing
                let extract_parent = extract_path
                    .parent()
                    .ok_or_else(|| {
                        anyhow!(
                            "Unable to find parent for {}",
                            extract_path.to_string_lossy()
                        )
                    })
                    .cancel(&canceled)?;
                if !fs::metadata(extract_parent).await.is_ok() {
                    fs::create_dir_all(extract_parent)
                        .await
                        .with_context(|| {
                            format!("Error creating dir {}", extract_parent.to_string_lossy())
                        })
                        .cancel(&canceled)?;
                }

                // Extract the file
                let mut writer = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&extract_path)
                    .await
                    .with_context(|| {
                        format!(
                            "Error opening destination file {}",
                            extract_path.to_string_lossy()
                        )
                    })
                    .cancel(&canceled)?;

                io::copy(&mut entry_reader, &mut writer)
                    .await
                    .with_context(|| {
                        format!("Error writing to file {}", extract_path.to_string_lossy())
                    })
                    .cancel(&canceled)?;

                // Send the update
                let extraction_count = extraction_count.fetch_add(1, Ordering::AcqRel) + 1;
                handle.emit_all("extraction_cur", extraction_count).unwrap();
            } else if !fs::metadata(&extract_path).await.is_ok() {
                fs::create_dir_all(&extract_path)
                    .await
                    .with_context(|| {
                        format!("Error creating dir {}", extract_path.to_string_lossy())
                    })
                    .cancel(&canceled)?;
            }

            Ok(())
        }));
    }

    while let Some(res) = futures.next().await {
        res.context("Error waiting for file extraction")?
            .context("Error extracting file")?;
    }

    handle.emit_all("extraction_complete", ()).unwrap();

    Ok(())
}

fn sanitize_override_path(path: &str) -> PathBuf {
    // Backslash replacement is done earlier.
    // We also want to remove the first element, as that corresponds to the 'overrides' directory.
    path.split('/')
        .map(sanitize_filename::sanitize)
        .skip(1)
        .collect()
}

pub struct FileStatusManager {
    status_map: RwLock<HashMap<String, RwLock<FileStatus>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    url: String,
    msg: Option<FileMsg>,
    cur_bytes: u64,
    total_bytes: u64,
    complete: bool,
}

#[derive(Clone, Serialize)]
pub struct FileMsg {
    msg: String,
    error: bool,
}

impl FileStatusManager {
    fn new() -> Self {
        Self {
            status_map: Default::default(),
        }
    }

    async fn setup(&self, handle: &AppHandle, url_list: Vec<Url>) {
        let mut status_map = self.status_map.write().await;

        status_map.clear();
        for url in url_list.iter() {
            let url = url.as_str();
            status_map.insert(url.to_string(), RwLock::new(FileStatus::initial(url)));
        }

        handle.emit_all("files_list", url_list).unwrap();
    }

    async fn put(&self, handle: &AppHandle, status: FileStatus) {
        let status_map = self.status_map.read().await;
        if let Some(status_holder) = status_map.get(&status.url) {
            let status = status.clone();
            let mut status_lock = status_holder.write().await;
            let old_status = status_lock.clone();
            let new_status = FileStatus {
                msg: status.msg.or(old_status.msg),
                ..status
            };
            *status_lock = new_status;
        }

        handle.emit_all("file_status", status).unwrap();
    }

    async fn get(&self, url: &str) -> Option<FileStatus> {
        let status_map = self.status_map.read().await;
        if let Some(holder) = status_map.get(url) {
            Some(holder.read().await.clone())
        } else {
            None
        }
    }
}

impl FileStatus {
    fn initial(url: &str) -> Self {
        Self {
            url: url.to_string(),
            msg: Some(FileMsg {
                msg: "Not started.".to_string(),
                error: false,
            }),
            cur_bytes: 0,
            total_bytes: 0,
            complete: false,
        }
    }

    fn from_str(
        url: &str,
        msg: &str,
        error: bool,
        cur_bytes: u64,
        total_bytes: u64,
        complete: bool,
    ) -> Self {
        Self {
            url: url.to_string(),
            msg: Some(FileMsg {
                msg: msg.to_string(),
                error,
            }),
            cur_bytes,
            total_bytes,
            complete,
        }
    }

    fn from_string(
        url: &str,
        msg: String,
        error: bool,
        cur_bytes: u64,
        total_bytes: u64,
        complete: bool,
    ) -> Self {
        Self {
            url: url.to_string(),
            msg: Some(FileMsg { msg, error }),
            cur_bytes,
            total_bytes,
            complete,
        }
    }

    fn from_empty(url: &str, cur_bytes: u64, total_bytes: u64, complete: bool) -> Self {
        Self {
            url: url.to_string(),
            msg: None,
            cur_bytes,
            total_bytes,
            complete,
        }
    }
}

async fn download_mods(
    client: Arc<Client>,
    handle: AppHandle,
    statuses: Arc<FileStatusManager>,
    manifest: ManifestJson,
    mods_path: PathBuf,
) -> anyhow::Result<()> {
    let url_list: anyhow::Result<Vec<Url>> = manifest
        .files
        .iter()
        .map(|f| Url::from_str(f.download_url.as_str()).context("Error parsing url"))
        .collect();
    let url_list = url_list?;
    statuses.setup(&handle, url_list).await;

    // Lazy concurrency limiter
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));

    let cancelled = Arc::new(AtomicBool::new(false));
    let files_complete = Arc::new(AtomicUsize::new(0));
    let mut futures = FuturesUnordered::<JoinHandle<anyhow::Result<()>>>::new();

    for file in manifest.files.iter() {
        let client = client.clone();
        let handle = handle.clone();
        let statuses = statuses.clone();
        let file = file.clone();
        let mods_path = mods_path.clone();
        let semaphore = semaphore.clone();
        let files_complete = files_complete.clone();
        let cancelled = cancelled.clone();

        futures.push(tokio::spawn(async move {
            let _permit = semaphore
                .acquire()
                .await
                .context("Error acquiring semaphore permit")?;

            // if cancelled.load(Ordering::Acquire) {
            //     return Ok(());
            // }

            let download_url = &file.download_url;
            let url = Url::from_str(download_url)
                .with_context(|| format!("Error parsing url {}", download_url))
                .cancel(&cancelled)?;
            let filename = &download_url[download_url.rfind('/').unwrap_or(0) + 1..];
            let mod_path = mods_path.join(filename);

            let mut writer = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&mod_path)
                .await
                .with_context(|| {
                    format!(
                        "Error opening destination mod file {}",
                        mod_path.to_string_lossy()
                    )
                })
                .cancel(&cancelled)?;

            download_file(
                client,
                handle,
                statuses,
                &url,
                &mut writer,
                Duration::from_secs(20),
                files_complete,
                cancelled.clone(),
            )
            .await
            .with_context(|| format!("Error downloading mod file {}", &url))
            .cancel(&cancelled)?;

            Ok(())
        }));
    }

    while let Some(res) = futures.next().await {
        res.context("Error waiting for file download")?
            .context("Error downloading file")?;
    }

    Ok(())
}

async fn download_file(
    client: Arc<Client>,
    handle: AppHandle,
    statuses: Arc<FileStatusManager>,
    url: &Url,
    output: &mut File,
    conn_timeout: Duration,
    files_complete: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let mut full_length = None;
    let mut offset = 0u64;

    loop {
        // if cancelled.load(Ordering::Acquire) {
        //     statuses
        //         .put(
        //             &handle,
        //             FileStatus::from_str(
        //                 url.as_str(),
        //                 "Cancelled.",
        //                 true,
        //                 offset,
        //                 full_length.unwrap_or(0),
        //                 false,
        //             ),
        //         )
        //         .await;
        //
        //     return Ok(());
        // }

        let res = download_file_part(
            client.clone(),
            handle.clone(),
            statuses.clone(),
            url,
            output,
            conn_timeout,
            &mut full_length,
            &mut offset,
            cancelled.clone(),
        )
        .await;

        match res {
            Ok(res) => break res,
            Err(err) => {
                statuses
                    .put(
                        &handle,
                        FileStatus::from_string(
                            url.as_str(),
                            err.to_string(),
                            true,
                            offset,
                            full_length.unwrap_or(0),
                            false,
                        ),
                    )
                    .await;
            }
        }
    }
    .with_context(|| format!("Error downloading mod file {}", &url))
    .cancel(&cancelled)?;

    statuses
        .put(
            &handle,
            FileStatus::from_str(
                url.as_str(),
                "Complete.",
                false,
                offset,
                full_length.unwrap_or(0),
                true,
            ),
        )
        .await;

    let files_complete = files_complete.fetch_add(1, Ordering::AcqRel) + 1;
    handle.emit_all("file_complete", files_complete).unwrap();

    Ok(())
}

async fn download_file_part(
    client: Arc<Client>,
    handle: AppHandle,
    statuses: Arc<FileStatusManager>,
    url: &Url,
    output: &mut File,
    conn_timeout: Duration,
    full_length: &mut Option<u64>,
    offset: &mut u64,
    cancelled: Arc<AtomicBool>,
) -> anyhow::Result<anyhow::Result<()>> {
    if cancelled.load(Ordering::Acquire) {
        return Ok(Err(anyhow!("Cancelled.")));
    }

    statuses
        .put(
            &handle,
            FileStatus::from_str(
                url.as_str(),
                "Connecting...",
                false,
                *offset,
                full_length.unwrap_or(0),
                false,
            ),
        )
        .await;

    let mut builder = client.get(url.clone());

    if *offset > 0u64 {
        builder = builder.header("range", format!("bytes={}-", offset));
    }

    let res = timeout(conn_timeout, builder.send())
        .await
        .context("Connection timeout")?
        .context("Error connecting to server")?;

    if (res.status().is_client_error() && res.status().as_u16() != 404)
        || res.status().is_server_error()
    {
        return Ok(Err(anyhow!(
            "Server gave bad response code: {}",
            res.status()
        )));
    }

    let length = res.content_length();
    let mut downloaded = 0u64;
    if full_length.is_none() {
        *full_length = length.map(|len| *offset + len);
    }

    let status = if let Some(length) = length {
        FileStatus::from_string(
            url.as_str(),
            format!("Connected. Downloading: {}", HumanBytes(length)),
            false,
            *offset,
            full_length.unwrap_or(0),
            false,
        )
    } else {
        FileStatus::from_str(
            url.as_str(),
            "Connected.",
            false,
            *offset,
            full_length.unwrap_or(0),
            false,
        )
    };
    statuses.put(&handle, status).await;

    let mut stream = res.bytes_stream();

    while let Some(item) = timeout(conn_timeout, stream.next())
        .await
        .context("Chunk download timeout")?
    {
        let chunk: Bytes = item.context("Error downloading byte chunk")?;

        output
            .write_all(chunk.chunk())
            .await
            .context("Error writing to file")?;

        let len = chunk.len() as u64;
        *offset += len;
        downloaded += len;

        statuses
            .put(
                &handle,
                FileStatus::from_empty(url.as_str(), *offset, full_length.unwrap_or(0), false),
            )
            .await;

        if cancelled.load(Ordering::Acquire) {
            return Ok(Err(anyhow!("Cancelled.")));
        }
    }

    if length.is_some_and(|length| downloaded < length) {
        bail!("Incomplete download");
    }

    Ok(Ok(()))
}
