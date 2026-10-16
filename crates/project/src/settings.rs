//! The user's settings file. Read once at startup; leaf writes a file of defaults the first time so it is
//! easy to find and edit. A file that does not parse is reported and the defaults are used instead.

use std::{
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// The editor's font family. "SF Mono" means the system's monospaced font.
    pub editor_font: String,
    pub editor_font_size: f32,
    /// Spaces a Tab inserts in code.
    pub tab_size: usize,
    pub soft_wrap: SoftWrap,
    /// The key hint line under the tree.
    pub key_hints: bool,
    /// Extra key bindings, applied after leaf's own (so they win).
    pub keymap: Vec<KeyOverride>,
}

/// Which files wrap long lines. Wrapping costs time on large files, so Markdown only by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoftWrap {
    #[default]
    Markdown,
    All,
    None,
}

/// One extra binding, e.g. `{ "key": "cmd-p", "action": "workspace::ToggleFinder", "context": "Workspace" }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyOverride {
    pub key: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            editor_font: "SF Mono".into(),
            editor_font_size: 13.,
            tab_size: 4,
            soft_wrap: SoftWrap::Markdown,
            key_hints: true,
            keymap: Vec::new(),
        }
    }
}

impl Settings {
    /// Whether a file in `language` wraps long lines.
    pub fn wraps(&self, markdown: bool) -> bool {
        match self.soft_wrap {
            SoftWrap::Markdown => markdown,
            SoftWrap::All => true,
            SoftWrap::None => false,
        }
    }
}

/// `~/Library/Application Support/leaf/settings.json`, or `LEAF_SETTINGS_PATH` for tests.
pub fn default_path() -> PathBuf {
    if let Some(path) = std::env::var_os("LEAF_SETTINGS_PATH") {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/leaf/settings.json")
}

/// The settings, or the defaults and why the file could not be used. A missing file is not an error.
pub fn load(path: &Path) -> (Settings, Option<String>) {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return (Settings::default(), None),
        Err(err) => return (Settings::default(), Some(format!("settings.json: {err}"))),
    };
    match serde_json::from_slice::<Settings>(&bytes) {
        Ok(settings) => match validate(&settings) {
            Ok(()) => (settings, None),
            Err(err) => (Settings::default(), Some(format!("settings.json: {err}"))),
        },
        Err(err) => (Settings::default(), Some(format!("settings.json: {err}"))),
    }
}

fn validate(settings: &Settings) -> Result<(), String> {
    if !(6. ..=72.).contains(&settings.editor_font_size) {
        return Err("editor_font_size must be between 6 and 72".into());
    }
    if !(1..=16).contains(&settings.tab_size) {
        return Err("tab_size must be between 1 and 16".into());
    }
    Ok(())
}

/// Writes the defaults when there is no settings file yet, so there is something to edit.
pub fn write_default_if_missing(path: &Path) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(&Settings::default()).map_err(io::Error::other)?;
    std::fs::write(path, json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_gives_the_defaults_without_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load(&dir.path().join("settings.json")),
            (Settings::default(), None)
        );
    }

    #[test]
    fn fields_left_out_keep_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{ "tab_size": 2, "soft_wrap": "all" }"#).unwrap();
        let (settings, error) = load(&path);
        assert_eq!(error, None);
        assert_eq!(settings.tab_size, 2);
        assert_eq!(settings.soft_wrap, SoftWrap::All);
        assert_eq!(settings.editor_font, "SF Mono");
    }

    #[test]
    fn a_broken_or_unknown_field_falls_back_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{ "tab_sise": 2 }"#).unwrap();
        let (settings, error) = load(&path);
        assert_eq!(settings, Settings::default());
        assert!(error.unwrap().contains("tab_sise"));
        std::fs::write(&path, r#"{ "tab_size": 0 }"#).unwrap();
        assert!(load(&path).1.unwrap().contains("tab_size"));
    }

    #[test]
    fn key_overrides_read_with_and_without_a_context() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{ "keymap": [ { "key": "cmd-p", "action": "workspace::ToggleFinder", "context": "Workspace" },
                             { "key": "f2", "action": "project_panel::Rename" } ] }"#,
        )
        .unwrap();
        let (settings, error) = load(&path);
        assert_eq!(error, None);
        assert_eq!(settings.keymap.len(), 2);
        assert_eq!(settings.keymap[1].context, None);
    }

    #[test]
    fn the_default_file_is_written_once_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("leaf/settings.json");
        write_default_if_missing(&path).unwrap();
        std::fs::write(&path, r#"{ "tab_size": 8 }"#).unwrap();
        write_default_if_missing(&path).unwrap();
        assert_eq!(load(&path).0.tab_size, 8);
    }

    #[test]
    fn soft_wrap_choices() {
        let mut settings = Settings::default();
        assert!(settings.wraps(true) && !settings.wraps(false));
        settings.soft_wrap = SoftWrap::All;
        assert!(settings.wraps(false));
        settings.soft_wrap = SoftWrap::None;
        assert!(!settings.wraps(true));
    }
}
