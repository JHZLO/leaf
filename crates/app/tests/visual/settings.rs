//! The Settings tab (Cmd+,): it opens beside the file tabs, its changes show at once and reach the
//! settings file.

use std::{path::PathBuf, time::Duration};

use gpui_kit::{
    AnyWindowHandle,
    component::{Theme, ThemeMode},
};
use subwy_project::settings::{Appearance, Settings};

use crate::check::{Check, fixture};

pub fn run(check: &mut Check) {
    let fixture = fixture();
    let window = check.open(Some(fixture.path()));
    check
        .cx
        .update(|cx| Theme::change(ThemeMode::Light, None, cx));
    check.keys(window, "down down right down down enter cmd-,");
    let shown = check.workspace(window, |w, cx| w.settings_shown(cx));
    check.expect_eq("cmd-, opens the Settings tab", shown, true);
    check.shot(window, "settings-1-editor");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq("no file is active behind it", active, None);

    // A change applies at once and reaches the file.
    change(check, window, |s| s.tab_size = 2);
    // The file is written a moment after the last change, off the main thread.
    check.cx.advance_clock(Duration::from_millis(500));
    check.cx.run_until_parked();
    std::thread::sleep(Duration::from_millis(100));
    check.cx.run_until_parked();
    let path = PathBuf::from(std::env::var("SUBWY_SETTINGS_PATH").expect("settings path"));
    let saved = std::fs::read_to_string(&path).unwrap_or_default();
    check.expect_eq(
        "the change is written to the settings file",
        saved.contains("\"tab_size\": 2"),
        true,
    );
    change(check, window, |s| s.tab_size = 4);

    // Cmd+Shift+[ goes back to the file; Cmd+W on the Settings tab closes it.
    check.keys(window, "cmd-shift-[");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "the tab before it is the file",
        active,
        Some(PathBuf::from("docs/notes.md")),
    );
    // Open editors take editor changes at once: no line numbers, whitespace shown.
    change(check, window, |s| {
        s.line_numbers = false;
        s.show_whitespace = true;
    });
    check.shot(window, "settings-2-editor-options");
    change(check, window, |s| {
        s.line_numbers = true;
        s.show_whitespace = false;
    });
    // Appearance: Dark turns the paper dark whatever macOS is set to.
    change(check, window, |s| s.appearance = Appearance::Dark);
    let dark = check.cx.update(|cx| Theme::global(cx).mode.is_dark());
    check.expect_eq("appearance dark turns the paper dark", dark, true);
    check.keys(window, "cmd-,");
    check.shot(window, "settings-3-dark");
    // The editor font survives a change of appearance.
    change(check, window, |s| s.editor_font = "Menlo".into());
    change(check, window, |s| s.appearance = Appearance::Light);
    let font = check
        .cx
        .update(|cx| Theme::global(cx).mono_font_family.to_string());
    check.expect_eq(
        "the editor font follows the settings",
        font,
        "Menlo".to_string(),
    );
    change(check, window, |s| s.editor_font = "SF Mono".into());
    let dark = check.cx.update(|cx| Theme::global(cx).mode.is_dark());
    check.expect_eq("appearance light turns it light", dark, false);
    change(check, window, |s| s.appearance = Appearance::System);
    check.keys(window, "cmd-shift-[");
    // The Tree page, its switches on light paper.
    check.keys(window, "cmd-,");
    let at = gpui_kit::point(gpui_kit::px(340.), gpui_kit::px(186.));
    check
        .cx
        .simulate_click(window, at, gpui_kit::Modifiers::default());
    check.shot(window, "settings-4-tree");
    check.keys(window, "cmd-w");
    keymap(check);
    tree_options(check);
    terminal_options(check);
    check.keys(window, "cmd-, cmd-w");
    let shown = check.workspace(window, |w, cx| w.settings_shown(cx));
    check.expect_eq("cmd-w closes the Settings tab", shown, false);
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "the file is in front again",
        active,
        Some(PathBuf::from("docs/notes.md")),
    );
}

/// Changes the settings as the Settings tab's fields do. The change applies once the update that
/// made it ends, as after a click.
fn change(check: &mut Check, window: AnyWindowHandle, edit: impl FnOnce(&mut Settings)) {
    check.update_workspace(window, |_, _, cx| subwy::update_settings(cx, edit));
    check.update_workspace(window, |_, _, _| {});
}

/// Open files with a single click, and the tree that stops following the active tab.
fn tree_options(check: &mut Check) {
    let fixture = fixture();
    let window = check.open(Some(fixture.path()));
    // Rows: the folder, docs, src, target, .gitignore, README.md.
    let readme = gpui_kit::point(gpui_kit::px(100.), gpui_kit::px(38. + 34. + 22. * 5. + 11.));
    change(check, window, |s| s.single_click_open = true);
    check
        .cx
        .simulate_click(window, readme, gpui_kit::Modifiers::default());
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "one click opens a file",
        active,
        Some(PathBuf::from("README.md")),
    );
    change(check, window, |s| {
        s.single_click_open = false;
        s.follow_active_file = false;
    });
    check.keys(window, "cmd-shift-o m a i n enter");
    let selected = check.workspace(window, |w, cx| w.tree_selection(cx));
    check.expect_eq(
        "the tree stays put when it does not follow",
        selected,
        Some(PathBuf::from("README.md")),
    );
    change(check, window, |s| s.follow_active_file = true);
}

/// With Open with the folder off, a new window starts no shell until Alt+F12.
fn terminal_options(check: &mut Check) {
    let (first, second) = (fixture(), fixture());
    let window = check.open(Some(first.path()));
    change(check, window, |s| s.terminal_on_start = false);
    let window = check.open(Some(second.path()));
    let tabs = check.workspace(window, |w, cx| w.terminal_tabs(cx));
    check.expect_eq("no shell starts with the folder", tabs, 0);
    check.keys(window, "alt-f12");
    let tabs = check.workspace(window, |w, cx| w.terminal_tabs(cx));
    check.expect_eq("alt-f12 still starts one", tabs, 1);
    change(check, window, |s| s.terminal_on_start = true);
}

/// The Keymap page: search an action, click its key, press the new one.
fn keymap(check: &mut Check) {
    let first = fixture();
    let window = check.open(Some(first.path()));
    let before = check.cx.update(|cx| subwy::settings(cx).keymap.clone());
    check.keys(window, "cmd-,");
    // Keymap is the sixth page in the sidebar.
    let page = gpui_kit::point(gpui_kit::px(340.), gpui_kit::px(272.));
    check
        .cx
        .simulate_click(window, page, gpui_kit::Modifiers::default());
    check.shot(window, "settings-5-keymap");
    // Search for Rename, click its key and press F2.
    click(check, window, 915., 149.);
    check.keys(window, "r e n a m e");
    click(check, window, 1120., 216.);
    check.shot(window, "settings-6-recording");
    check.keys(window, "f2");
    let keymap = check.cx.update(|cx| subwy::settings(cx).keymap.clone());
    let given = keymap
        .iter()
        .any(|o| o.action == "project_panel::Rename" && o.key == "f2" && !o.unbind);
    check.expect_eq("a recorded key goes into the settings", given, true);
    check.shot(window, "settings-7-recorded");
    check.keys(window, "cmd-w");
    // Each try in a window of its own, the tree in front: Shift+F6 no longer renames (x starts a
    // speed search instead), F2 does.
    let (old, new) = (fixture(), fixture());
    let window = check.open(Some(old.path()));
    check.keys(window, "end shift-f6 x enter");
    let readme = old.path().join("README.md");
    check.wait_until(|check| {
        check.workspace(window, |w, cx| w.active_file(cx)) == Some(PathBuf::from("README.md"))
    });
    check.expect_eq("the old key is gone", readme.is_file(), true);
    let window = check.open(Some(new.path()));
    check.keys(window, "end f2 cmd-a n e w . m d enter");
    let renamed = new.path().join("new.md");
    check.wait_until(|_| renamed.is_file());
    check.expect_eq("the new key renames", renamed.is_file(), true);
    // Reset: the own key is back.
    let back = fixture();
    let window = check.open(Some(back.path()));
    change(check, window, |s| s.keymap = before);
    check.keys(window, "end shift-f6 cmd-a i n d e x . m d enter");
    let index = back.path().join("index.md");
    check.wait_until(|_| index.is_file());
    check.expect_eq("a reset brings the own key back", index.is_file(), true);
}

fn click(check: &mut Check, window: AnyWindowHandle, x: f32, y: f32) {
    let at = gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
    check
        .cx
        .simulate_click(window, at, gpui_kit::Modifiers::default());
    check.cx.run_until_parked();
}
