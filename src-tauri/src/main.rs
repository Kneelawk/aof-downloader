#![feature(is_some_and)]
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[macro_use]
extern crate anyhow;

#[macro_use]
extern crate log;

use std::sync::Arc;

mod download;
mod json;
mod select;
mod utils;

fn main() {
    dotenv::dotenv().ok();
    env_logger::init();

    tauri::Builder::default()
        .manage(Arc::new(download::DownloaderState::new()))
        .invoke_handler(tauri::generate_handler![
            select::modpack_select,
            select::path_is_valid,
            download::get_status,
            download::human_bytes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
