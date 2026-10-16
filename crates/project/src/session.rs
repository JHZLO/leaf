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

/// `~/Library/Application Support/leaf/session.json`, or `LEAF_SESSION_PATH` so tests and
/// scripted runs never touch the real session.
pub fn default_path() -> PathBuf {
    if let Some(path) = std::env::var_os("LEAF_SESSION_PATH") {
        return PathBuf::from(path);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Session {
        Session {
            root: Some(PathBuf::from("/Users/me/notes")),
            expanded: vec![PathBuf::from("docs"), PathBuf::from("docs/guide")],
            selected: Some(PathBuf::from("docs/guide/a.md")),
            tabs: vec![
                TabState {
                    path: PathBuf::from("docs/guide/a.md"),
                    line: 11,
                    column: 4,
                },
                TabState {
                    path: PathBuf::from("readme.md"),
                    line: 0,
                    column: 0,
                },
            ],
            active_tab: Some(1),
            tree_width: Some(260.5),
            preview: true,
            zoom: 125,
            hide_key_hints: true,
        }
    }

    #[test]
    fn a_saved_session_loads_back_the_same() {
        let dir = tempfile::tempdir().unwrap();
        // The folder under Application Support may not exist yet.
        let path = dir.path().join("leaf/session.json");
        save(&path, &sample()).unwrap();
        assert_eq!(load(&path), sample());
    }

    #[test]
    fn a_missing_session_loads_as_the_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("session.json")), Session::default());
    }

    #[test]
    fn a_corrupt_session_loads_as_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        for bytes in [
            &b"not json"[..],
            b"{\"root\": \"/Users/me/no",
            b"{\"zoom\": \"large\"}",
            b"[]",
            b"\xff\xfe\x00",
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(load(&path), Session::default());
        }
    }

    #[test]
    fn missing_fields_take_their_defaults_and_unknown_ones_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(
            &path,
            br#"{"zoom": 150, "tabs": [{"path": "a.md"}], "from_a_newer_leaf": 1}"#,
        )
        .unwrap();
        let session = load(&path);
        assert_eq!(session.zoom, 150);
        assert_eq!(
            session.tabs,
            [TabState {
                path: PathBuf::from("a.md"),
                line: 0,
                column: 0,
            }]
        );
        assert_eq!(session.root, None);
        assert!(!session.preview);
    }
}
