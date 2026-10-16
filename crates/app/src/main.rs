//! leaf: a light, keyboard-first text and Markdown editor for macOS.

// The theme JSON in theme.rs is one large `json!` literal.
#![recursion_limit = "256"]

mod editor_tabs;
mod empty_state;
mod keymap;
mod menus;
mod preview;
mod project_panel;
mod status_bar;
mod theme;
mod title_bar;
mod workspace;

use std::path::PathBuf;

fn main() {
    let arg = std::env::args_os().nth(1).map(PathBuf::from);
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        keymap::init(cx);
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
