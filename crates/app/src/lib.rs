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
mod status_bar;
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

/// Registers leaf's settings, theme and keys; what both the app and the UI tests start from.
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
}

/// Runs the app. `arg` is a folder to open, or a file to open inside its folder.
pub fn run(arg: Option<PathBuf>) {
    // The default icon set, which gpui-kit's icons (chevrons, dialog buttons) draw from.
    // A file of defaults the first time, so the settings are easy to find and edit.
    if let Err(err) = settings::write_default_if_missing(&settings::default_path()) {
        eprintln!("leaf: no settings file written: {err}");
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            init(cx);
            menus::init(cx);
            cx.on_action(|_: &keymap::Quit, cx| cx.quit());
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            workspace::open(arg, cx);
            // Screenshot runs set LEAF_BACKGROUND so the window never takes the keyboard from someone typing.
            if std::env::var_os("LEAF_BACKGROUND").is_none() {
                cx.activate(true);
            }
        });
}
