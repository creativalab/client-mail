//! Where Ferrowl keeps its files.
//!
//! On desktop these come from the platform's standard directories. Mobile
//! apps call [`set`] once at startup with directories inside their sandbox,
//! before anything else in the core runs.

use std::path::PathBuf;
use std::sync::OnceLock;

static DIRS: OnceLock<(PathBuf, PathBuf)> = OnceLock::new();

/// Sets the data and configuration directories. Only the first call counts.
pub fn set(data: PathBuf, config: PathBuf) {
    let _ = DIRS.set((data, config));
}

/// Cache, local mailbox, OpenPGP keys.
pub fn data_dir() -> Option<PathBuf> {
    match DIRS.get() {
        Some((data, _)) => Some(data.clone()),
        None => Some(dirs::data_local_dir()?.join("ferrowl")),
    }
}

/// Settings (`config.toml`).
pub fn config_dir() -> Option<PathBuf> {
    match DIRS.get() {
        Some((_, config)) => Some(config.clone()),
        None => Some(dirs::config_dir()?.join("ferrowl")),
    }
}
