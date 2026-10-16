//! What leaf reopens with: the folder, the tree, the tabs, the zoom.

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub root: Option<PathBuf>,
    /// Open folders, relative to `root`.
    pub expanded: Vec<PathBuf>,
    pub selected: Option<PathBuf>,
    pub tabs: Vec<TabState>,
    pub active_tab: Option<usize>,
    pub tree_width: Option<f32>,
    pub preview: bool,
    /// UI zoom in percent; 0 means the default (100).
    pub zoom: u32,
    pub hide_key_hints: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabState {
    /// Relative to the session's root.
    pub path: PathBuf,
    /// Zero-based cursor position.
    pub line: u32,
    pub column: u32,
}

/// `~/Library/Application Support/leaf/session.json`.
pub fn default_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/leaf/session.json")
}

/// A missing or unreadable session is an empty one: losing it must never block startup.
pub fn load(path: &Path) -> Session {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, session: &Session) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(session).map_err(io::Error::other)?;
    std::fs::write(path, json)
}
