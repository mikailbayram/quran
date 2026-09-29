//! Platform-independent core of the Quran app: data models, API client,
//! offline cache and settings. UI front-ends (GTK today, mobile later) sit on top.

pub mod api;
pub mod models;
pub mod settings;
pub mod store;
pub mod text;

pub use api::{Client, FontFile, VerseQuery};
pub use models::*;
pub use settings::*;
pub use store::Store;

use std::path::PathBuf;
use std::sync::Arc;

/// Open the store in the user's data directory and build a client.
pub fn open_default() -> std::io::Result<Client> {
    let dirs = directories::ProjectDirs::from("com", "quran", "quran-desktop")
        .ok_or_else(|| std::io::Error::other("no home directory"))?;
    let data: PathBuf = dirs.data_dir().to_path_buf();
    std::fs::create_dir_all(&data)?;
    let store = Store::open(&data.join("quran.sqlite3")).map_err(std::io::Error::other)?;
    Ok(Client::new(Arc::new(store), data))
}
