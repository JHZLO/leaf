//! leaf: a light, keyboard-first text and Markdown editor for macOS.
//! A library so the offscreen UI tests in tests/ can build the same window; `main.rs` only calls [`run`].

// The theme JSON in theme.rs is one large `json!` literal.
#![recursion_limit = "256"]

mod editor_tabs;
mod empty_state;
mod finder;
pub mod keymap;
mod menus;
mod preview;
mod project_panel;
mod search_panel;
mod status_bar;
mod terminal_panel;
mod terminal_tabs;
pub mod theme;
mod title_bar;
pub mod workspace;

use std::path::PathBuf;

use gpui_kit::{App, Global};
use leaf_project::settings::{self, Settings};

/// The user's settings, read once at startup, and why the file could not be used, if it could not.
pub struct UserSettings {
    pub settings: Settings,
    pub error: Option<String>,
}

impl Global for UserSettings {}

pub fn settings(cx: &App) -> &Settings {
    &cx.global::<UserSettings>().settings
}

/// Registers leaf's settings, theme, keys and the shared session; what both the app and the UI
/// tests start from.
pub fn init(cx: &mut App) {
    let (settings, error) = settings::load(&settings::default_path());
    let overrides = settings.keymap.clone();
    cx.set_global(UserSettings { settings, error });
    gpui_kit::init(cx);
    theme::init(cx);
    keymap::init(cx);
    // Bound after leaf's own keys so the user's win.
    if let Some(problem) = keymap::bind_overrides(&overrides, cx) {
        let state = cx.global_mut::<UserSettings>();
        state.error.get_or_insert(problem);
    }
    workspace::init(cx);
}

/// Runs the app. `arg` is a folder to open, or a file to open inside its folder.
pub fn run(arg: Option<PathBuf>) {
    // A file of defaults the first time, so the settings are easy to find and edit.
    if let Err(err) = settings::write_default_if_missing(&settings::default_path()) {
        eprintln!("leaf: no settings file written: {err}");
    }
    // The default icon set, which gpui-kit's icons (chevrons, dialog buttons) draw from.
    let app = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    // Paths from Finder's Open With, `open -a leaf` and the leaf command. When they start leaf,
    // macOS hands them over before launching finishes, so they wait in the channel until then.
    let (opened, requests) = smol::channel::unbounded::<Vec<String>>();
    app.on_open_urls(move |urls| {
        let _ = opened.try_send(urls);
    });
    app.run(move |cx| {
        init(cx);
        menus::init(cx);
        cx.on_action(|_: &keymap::Quit, cx| cx.quit());
        cx.on_window_closed(|cx, id| {
            if cx.windows().is_empty() {
                cx.quit();
            } else {
                workspace::window_closed(id, cx);
            }
        })
        .detach();
        let mut paths: Vec<PathBuf> = arg.into_iter().collect();
        while let Ok(urls) = requests.try_recv() {
            paths.extend(urls.iter().filter_map(|url| path_from_url(url)));
        }
        workspace::open_at_start(paths, cx);
        cx.spawn(async move |cx| {
            while let Ok(urls) = requests.recv().await {
                cx.update(|cx| {
                    for path in urls.iter().filter_map(|url| path_from_url(url)) {
                        workspace::open_path(&path, cx);
                    }
                });
            }
        })
        .detach();
        // Screenshot runs set LEAF_BACKGROUND so the window never takes the keyboard from someone typing.
        if std::env::var_os("LEAF_BACKGROUND").is_none() {
            cx.activate(true);
        }
    });
}

/// The path in a `file://` URL as macOS sends it (percent-encoded, folders end in a slash).
/// Other URLs are not leaf's.
fn path_from_url(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let mut bytes = Vec::with_capacity(rest.len());
    let mut input = rest.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let hex = [input.next()?, input.next()?];
            let hex = std::str::from_utf8(&hex).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    if bytes.len() > 1 && bytes.ends_with(b"/") {
        bytes.pop();
    }
    use std::os::unix::ffi::OsStringExt as _;
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

#[cfg(test)]
mod tests {
    use super::path_from_url;
    use std::path::PathBuf;

    #[test]
    fn a_file_url_becomes_its_path() {
        assert_eq!(
            path_from_url("file:///Users/me/notes/"),
            Some(PathBuf::from("/Users/me/notes"))
        );
        assert_eq!(
            path_from_url("file:///Users/me/My%20Notes/todo.md"),
            Some(PathBuf::from("/Users/me/My Notes/todo.md"))
        );
        assert_eq!(
            path_from_url("file://localhost/tmp/a%25b"),
            Some(PathBuf::from("/tmp/a%b"))
        );
        // Korean names arrive as percent-encoded UTF-8.
        assert_eq!(
            path_from_url("file:///Users/me/%ED%95%9C%EA%B8%80.md"),
            Some(PathBuf::from("/Users/me/한글.md"))
        );
        assert_eq!(path_from_url("file:///"), Some(PathBuf::from("/")));
    }

    /// Here rather than in theme.rs: a test there trips the recursion limit its `json!` needs.
    #[test]
    fn the_editor_selection_blends_to_the_selection_color() {
        use crate::theme::{DARK, LIGHT, SELECTION_ALPHA, editor_selection};
        for p in [&LIGHT, &DARK] {
            let given = editor_selection(p);
            for shift in [16, 8, 0] {
                let channel = |value: u32| ((value >> shift) & 0xFF) as f32;
                let blended =
                    SELECTION_ALPHA * channel(given) + (1. - SELECTION_ALPHA) * channel(p.paper);
                assert!(
                    (blended - channel(p.sel_bg)).abs() <= 1.,
                    "{given:06X} at {shift}"
                );
            }
        }
    }

    #[test]
    fn other_urls_are_not_paths() {
        assert_eq!(path_from_url("https://example.com/a.md"), None);
        assert_eq!(path_from_url("leaf://open"), None);
        assert_eq!(path_from_url("file://server/share"), None);
        assert_eq!(path_from_url("file:///bad%2"), None);
        assert_eq!(path_from_url("file:///bad%zz"), None);
    }
}
