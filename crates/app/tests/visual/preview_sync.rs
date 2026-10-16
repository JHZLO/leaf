//! The preview follows the editor; local images show.

use std::{fs, path::PathBuf};

use gpui_kit::{test::TestWindowExt as _, *};

use crate::check::Check;

pub fn run(check: &mut Check) {
    follows_the_editor(check);
}

/// Draws a few frames: the editor scrolls in one, the preview asks where the line is in the next,
/// and moves in the one after.
fn settle(check: &mut Check, window: AnyWindowHandle) {
    for _ in 0..4 {
        check
            .cx
            .update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        check.cx.run_until_parked();
    }
}

/// Where the top of the preview's page is, in window pixels; it moves up as the preview scrolls.
fn page_top(check: &mut Check, window: AnyWindowHandle) -> f32 {
    check
        .cx
        .update_window(window, |_, window, _| {
            window
                .try_find("preview-page")
                .map_or(f32::NAN, |page| f32::from(page.bounds().top()))
        })
        .expect("window")
}

fn follows_the_editor(check: &mut Check) {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut text = String::from("# A long read\n\nSections to scroll through.\n");
    for i in 1..=80 {
        text.push_str(&format!(
            "\n## Section {i}\n\nParagraph {i} has a few words in it, enough for a line.\n\n\
             - item {i}a\n- item {i}b\n"
        ));
    }
    fs::write(dir.path().join("long.md"), text).expect("write");
    let window = check.open(Some(dir.path()));

    check.keys(window, "down enter cmd-shift-v");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "preview sync: enter opens long.md",
        active,
        Some(PathBuf::from("long.md")),
    );
    settle(check, window);
    let top = page_top(check, window);

    check.keys(window, "pagedown pagedown pagedown");
    settle(check, window);
    let middle = top - page_top(check, window);
    check.shot(window, "preview-sync-middle");
    check.expect_eq(
        "preview sync: page down scrolls the preview",
        middle > 0.,
        true,
    );

    check.keys(window, "cmd-down");
    settle(check, window);
    let end = top - page_top(check, window);
    check.shot(window, "preview-sync-end");
    check.expect_eq(
        "preview sync: the end of the file scrolls it further",
        end > middle,
        true,
    );

    check.keys(window, "cmd-up");
    settle(check, window);
    let back = top - page_top(check, window);
    check.expect_eq("preview sync: the start scrolls it back", back, 0.);
}
