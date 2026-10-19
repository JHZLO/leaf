//! Phase 1: walk the tree, copy paths, leaf through, the preview, a code file, zoom, the empty window.

use std::{path::PathBuf, time::Duration};

use gpui_kit::component::{Theme, ThemeMode};

use crate::check::{Check, fixture};

pub fn run(check: &mut Check) {
    let fixture = fixture();

    // The tree: walk into docs/guide with the arrow keys; the route follows the selection.
    let window = check.open(Some(fixture.path()));
    check.keys(window, "down down right down right down down");
    let selected = check.workspace(window, |w, cx| w.tree_selection(cx));
    check.expect_eq(
        "arrow keys select docs/guide/paths.md",
        selected,
        Some(PathBuf::from("docs/guide/paths.md")),
    );
    check.keys(window, "cmd-alt-shift-c");
    let copied = check.clipboard();
    check.expect_eq(
        "relative path from the tree",
        copied,
        Some("docs/guide/paths.md".into()),
    );
    check.shot(window, "1-tree-route-copied");

    // Space opens it as the leaf-through tab and keeps focus in the tree.
    check.keys(window, "space");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "space opens the selection",
        active,
        Some(PathBuf::from("docs/guide/paths.md")),
    );
    check.shot(window, "2-leaf-through");

    // Escape moves to the editor; the copy now carries the cursor line.
    check.keys(window, "escape cmd-alt-shift-c");
    let copied = check.clipboard();
    check.expect_eq(
        "relative path and line from the editor",
        copied,
        Some("docs/guide/paths.md:1".into()),
    );

    check.keys(window, "cmd-shift-v");
    check.shot(window, "3-preview");

    check
        .cx
        .update(|cx| Theme::change(ThemeMode::Light, None, cx));
    check.shot(window, "4-preview-light");
    check.keys(window, "cmd-shift-v");

    // A code file opens full width.
    check.keys(window, "cmd-1 down down right down enter");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "enter opens src/main.rs",
        active,
        Some(PathBuf::from("src/main.rs")),
    );
    check.keys(window, "cmd-=");
    check.shot(window, "5-code-zoomed-light");
    check.keys(window, "cmd-0");

    check
        .cx
        .update(|cx| Theme::change(ThemeMode::Dark, None, cx));
    // A key from the settings file (cmd-p opens the finder) works like a built-in one.
    check.keys(window, "cmd-p n o t e s enter");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "a key bound in settings.json opens the finder",
        active,
        Some(PathBuf::from("docs/notes.md")),
    );

    // The first window's folder is saved as recent; the empty window offers it and Enter opens it.
    // While a window shows that folder, Enter would bring that window forward instead, so it closes.
    check.cx.advance_clock(Duration::from_secs(2));
    check
        .cx
        .update_window(window, |_, window, _| window.remove_window())
        .expect("window");
    let empty = check.open(None);
    check.shot(empty, "6-empty-dark");
    check.keys(empty, "enter");
    let root = check.workspace(empty, |w, cx| w.root(cx));
    check.expect_eq(
        "enter opens the recent folder",
        root,
        Some(fixture.path().canonicalize().expect("root")),
    );
}
