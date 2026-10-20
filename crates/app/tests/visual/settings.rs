//! The Settings tab (Cmd+,): it opens beside the file tabs, its changes show at once and reach the
//! settings file.

use std::{path::PathBuf, time::Duration};

use gpui_kit::component::{Theme, ThemeMode};

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
    check
        .cx
        .update(|cx| subwy::update_settings(cx, |s| s.tab_size = 2));
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
    check
        .cx
        .update(|cx| subwy::update_settings(cx, |s| s.tab_size = 4));

    // Cmd+Shift+[ goes back to the file; Cmd+W on the Settings tab closes it.
    check.keys(window, "cmd-shift-[");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "the tab before it is the file",
        active,
        Some(PathBuf::from("docs/notes.md")),
    );
    // Open editors take editor changes at once: no line numbers, whitespace shown.
    check.cx.update(|cx| {
        subwy::update_settings(cx, |s| {
            s.line_numbers = false;
            s.show_whitespace = true;
        })
    });
    check.shot(window, "settings-2-editor-options");
    check.cx.update(|cx| {
        subwy::update_settings(cx, |s| {
            s.line_numbers = true;
            s.show_whitespace = false;
        })
    });
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
