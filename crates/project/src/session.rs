//! What leaf reopens with: each window's folder, tree and tabs, the zoom and the recent folders.

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// The whole session file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    /// The windows open when leaf last saved, in the order they were opened.
    pub windows: Vec<WindowState>,
    /// UI zoom in percent, shared by every window; 0 means the default (100).
    pub zoom: u32,
    /// Folders opened before, newest first.
    pub recent: Vec<PathBuf>,
}

/// One window: its folder, the tree and the tabs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowState {
    pub root: Option<PathBuf>,
    /// Open folders, relative to `root`.
    pub expanded: Vec<PathBuf>,
    pub selected: Option<PathBuf>,
    pub tabs: Vec<TabState>,
    pub active_tab: Option<usize>,
    pub tree_width: Option<f32>,
    pub preview: bool,
}

impl Session {
    /// The saved state of the window that had `root` open.
    pub fn window(&self, root: &Path) -> Option<&WindowState> {
        self.windows
            .iter()
            .find(|w| w.root.as_deref() == Some(root))
    }
}

/// How many folders the recent list keeps.
pub const MAX_RECENT: usize = 8;

/// Puts `root` first in `recent`, without duplicates, keeping at most [`MAX_RECENT`].
pub fn remember(recent: &mut Vec<PathBuf>, root: PathBuf) {
    recent.retain(|r| *r != root);
    recent.insert(0, root);
    recent.truncate(MAX_RECENT);
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabState {
    /// Relative to the window's root.
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
        .and_then(|bytes| parse(&bytes))
        .unwrap_or_default()
}

/// Reads both layouts: the list of windows, and the single window leaf saved before it had several
/// (the window's fields at the top level, beside the zoom and the recent folders).
fn parse(bytes: &[u8]) -> Option<Session> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let mut session: Session = serde_json::from_value(value.clone()).ok()?;
    if value.get("windows").is_none() {
        let window: WindowState = serde_json::from_value(value).ok()?;
        if window.root.is_some() {
            session.windows.push(window);
        }
    }
    Some(session)
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

    #[test]
    fn remember_puts_the_folder_first_without_duplicates() {
        let mut recent: Vec<PathBuf> = (0..MAX_RECENT)
            .map(|i| PathBuf::from(format!("/f{i}")))
            .collect();
        remember(&mut recent, PathBuf::from("/f3"));
        assert_eq!(recent[0], PathBuf::from("/f3"));
        assert_eq!(recent.len(), MAX_RECENT);
        assert_eq!(
            recent
                .iter()
                .filter(|r| r.as_path() == Path::new("/f3"))
                .count(),
            1
        );
        remember(&mut recent, PathBuf::from("/new"));
        assert_eq!(recent[0], PathBuf::from("/new"));
        assert_eq!(recent.len(), MAX_RECENT);
    }

    fn notes() -> WindowState {
        WindowState {
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
        }
    }

    fn sample() -> Session {
        Session {
            windows: vec![
                notes(),
                WindowState {
                    root: Some(PathBuf::from("/Users/me/code")),
                    tabs: vec![TabState {
                        path: PathBuf::from("src/main.rs"),
                        line: 3,
                        column: 0,
                    }],
                    active_tab: Some(0),
                    ..Default::default()
                },
            ],
            zoom: 125,
            recent: vec![
                PathBuf::from("/Users/me/code"),
                PathBuf::from("/Users/me/notes"),
            ],
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
    fn the_file_lists_each_window_beside_the_shared_fields() {
        let json: serde_json::Value = serde_json::to_value(sample()).unwrap();
        assert_eq!(json["windows"][1]["root"], "/Users/me/code");
        assert_eq!(json["windows"][0]["tabs"][0]["line"], 11);
        assert_eq!(json["zoom"], 125);
        assert_eq!(json["recent"][0], "/Users/me/code");
        // Each window keeps only its own state.
        assert!(json["windows"][0].get("recent").is_none());
        assert!(json["windows"][0].get("zoom").is_none());
    }

    #[test]
    fn a_window_is_found_by_its_folder() {
        let session = sample();
        assert_eq!(session.window(Path::new("/Users/me/notes")), Some(&notes()));
        assert_eq!(session.window(Path::new("/Users/me/other")), None);
    }

    #[test]
    fn a_single_window_file_from_before_loads_as_one_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(
            &path,
            br#"{
              "root": "/Users/me/notes",
              "expanded": ["docs", "docs/guide"],
              "selected": "docs/guide/a.md",
              "tabs": [
                {"path": "docs/guide/a.md", "line": 11, "column": 4},
                {"path": "readme.md", "line": 0, "column": 0}
              ],
              "active_tab": 1,
              "tree_width": 260.5,
              "preview": true,
              "zoom": 125,
              "recent": ["/Users/me/notes"]
            }"#,
        )
        .unwrap();
        assert_eq!(
            load(&path),
            Session {
                windows: vec![notes()],
                zoom: 125,
                recent: vec![PathBuf::from("/Users/me/notes")],
            }
        );
    }

    #[test]
    fn a_file_from_before_without_a_folder_has_no_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        std::fs::write(&path, br#"{"zoom": 110, "recent": ["/Users/me/notes"]}"#).unwrap();
        let session = load(&path);
        assert!(session.windows.is_empty());
        assert_eq!(session.zoom, 110);
        assert_eq!(session.recent, [PathBuf::from("/Users/me/notes")]);
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
            b"{\"windows\": 3}",
            b"{\"root\": 3}",
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
            br#"{"zoom": 150, "windows": [{"root": "/r", "tabs": [{"path": "a.md"}], "from_a_newer_leaf": 1}], "also_newer": true}"#,
        )
        .unwrap();
        let session = load(&path);
        assert_eq!(session.zoom, 150);
        assert_eq!(
            session.windows,
            [WindowState {
                root: Some(PathBuf::from("/r")),
                tabs: vec![TabState {
                    path: PathBuf::from("a.md"),
                    line: 0,
                    column: 0,
                }],
                ..Default::default()
            }]
        );
        assert!(session.recent.is_empty());
    }
}
