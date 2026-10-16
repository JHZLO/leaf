//! leaf: a light, keyboard-first text and Markdown editor for macOS.
//! A library so the offscreen UI tests in tests/ can build the same window; `main.rs` only calls [`run`].

// The theme JSON in theme.rs is one large `json!` literal.
#![recursion_limit = "256"]

mod editor_tabs;
mod empty_state;
pub mod keymap;
mod menus;
mod preview;
mod project_panel;
mod status_bar;
pub mod theme;
mod title_bar;
pub mod workspace;

use std::path::PathBuf;

/// Registers leaf's theme and keys; what both the app and the UI tests start from.
pub fn init(cx: &mut gpui_kit::App) {
    gpui_kit::init(cx);
    theme::init(cx);
    keymap::init(cx);
}

/// Runs the app. `arg` is a folder to open, or a file to open inside its folder.
pub fn run(arg: Option<PathBuf>) {
    // The default icon set, which gpui-kit's icons (chevrons, dialog buttons) draw from.
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
