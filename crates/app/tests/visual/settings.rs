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
