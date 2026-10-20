//! The user's settings file. Read once at startup; subwy writes a file of defaults the first time so it is
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
    /// Tab inserts a tab character instead of spaces.
    pub hard_tabs: bool,
    pub soft_wrap: SoftWrap,
    /// Line numbers in the editor's gutter.
    pub line_numbers: bool,
    /// Spaces and tabs drawn as faint marks.
    pub show_whitespace: bool,
    /// Faint lines down each indent level.
    pub indent_guides: bool,
    /// Code blocks fold from the gutter.
    pub folding: bool,
    /// Light, dark, or as macOS is set.
    pub appearance: Appearance,
    /// The key hint line under the tree.
    pub key_hints: bool,
    /// Gitignored items in the tree, dimmed; `false` leaves them out.
    pub show_ignored: bool,
    /// Extra key bindings, applied after subwy's own (so they win).
    pub keymap: Vec<KeyOverride>,
}

/// Light or dark paper; `System` follows macOS, as subwy always did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
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
            hard_tabs: false,
            soft_wrap: SoftWrap::Markdown,
            line_numbers: true,
            show_whitespace: false,
            indent_guides: true,
            folding: true,
            appearance: Appearance::System,
            key_hints: true,
            show_ignored: true,
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

/// `~/Library/Application Support/subwy/settings.json`, or `SUBWY_SETTINGS_PATH` for tests.
pub fn default_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SUBWY_SETTINGS_PATH") {
        return PathBuf::from(path);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/subwy/settings.json")
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

/// Writes the settings the Settings tab changed: to a file beside it first, then in its place, so
/// a crash in between leaves the old file whole.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(io::Error::other)?;
    let temp = path.with_extension("json.saving");
    std::fs::write(&temp, json)?;
    std::fs::rename(&temp, path)
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

/// Copies the settings and session subwy kept while it was called leaf into its own folder, the
/// first time it starts, so the rename keeps the keys, windows and recent folders. leaf's files stay.
pub fn adopt_leaf_files(home: &Path) -> io::Result<()> {
    let support = home.join("Library/Application Support");
    let (old, new) = (support.join("leaf"), support.join("subwy"));
    if new.exists() || !old.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(&new)?;
    for name in ["settings.json", "session.json"] {
        let from = old.join(name);
        if from.is_file() {
            std::fs::copy(&from, new.join(name))?;
        }
    }
    Ok(())
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
        let path = dir.path().join("subwy/settings.json");
        write_default_if_missing(&path).unwrap();
        std::fs::write(&path, r#"{ "tab_size": 8 }"#).unwrap();
        write_default_if_missing(&path).unwrap();
        assert_eq!(load(&path).0.tab_size, 8);
    }

    #[test]
    fn leaf_files_come_over_once() {
        let home = tempfile::tempdir().unwrap();
        let support = home.path().join("Library/Application Support");
        std::fs::create_dir_all(support.join("leaf")).unwrap();
        std::fs::write(support.join("leaf/settings.json"), r#"{ "tab_size": 8 }"#).unwrap();
        std::fs::write(support.join("leaf/session.json"), "{}").unwrap();
        adopt_leaf_files(home.path()).unwrap();
        assert_eq!(load(&support.join("subwy/settings.json")).0.tab_size, 8);
        assert!(support.join("subwy/session.json").is_file());
        assert!(support.join("leaf/settings.json").is_file());
        // Once subwy has its own folder, leaf's files are left alone.
        std::fs::write(support.join("leaf/settings.json"), r#"{ "tab_size": 2 }"#).unwrap();
        adopt_leaf_files(home.path()).unwrap();
        assert_eq!(load(&support.join("subwy/settings.json")).0.tab_size, 8);
    }

    #[test]
    fn nothing_to_bring_over_without_leaf() {
        let home = tempfile::tempdir().unwrap();
        adopt_leaf_files(home.path()).unwrap();
        assert!(
            !home
                .path()
                .join("Library/Application Support/subwy")
                .exists()
        );
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

    #[test]
    fn saved_settings_read_back_and_leave_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subwy/settings.json");
        let settings = Settings {
            tab_size: 2,
            key_hints: false,
            ..Settings::default()
        };
        save(&path, &settings).unwrap();
        assert_eq!(load(&path), (settings, None));
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["settings.json"]);
    }
}
