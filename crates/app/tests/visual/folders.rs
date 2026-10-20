//! Several folders in one window (IntelliJ's modules): the tree shows them side by side at the top,
//! a file in the second folder opens and copies relative to that folder, the session keeps the
//! folders, and a folder row's menu takes its folder out again.

use std::{path::PathBuf, time::Duration};

use gpui_kit::{Modifiers, MouseButton, point, px};
use subwy_project::session;

use crate::check::{Check, fixture};

/// Tree geometry at 100% zoom: rows start under the title bar (38) and the tree's header (34).
const FIRST_ROW: f32 = 38. + 34.;
const ROW: f32 = 22.;

pub fn run(check: &mut Check) {
    let first = fixture();
    let first_root = first.path().canonicalize().expect("root");
    let second = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(second.path().join("queries")).expect("dirs");
    std::fs::write(second.path().join("queries/query.sql"), "select 1;\n").expect("write");
    let second_root = second.path().canonicalize().expect("root");
    let key = |root: &PathBuf| PathBuf::from(root.file_name().expect("name"));
    let (first_key, second_key) = (key(&first_root), key(&second_root));

    let window = check.open(Some(&first_root));
    // What Cmd+O's This Window or Add Folder… does.
    let added = second_root.clone();
    check.update_workspace(window, move |w, window, cx| w.add_folder(added, window, cx));
    let rows = check.workspace(window, |w, cx| w.tree_rows(cx));
    check.expect_eq(
        "both folders sit at the top, the first one first",
        (
            rows.first().cloned(),
            rows.contains(&second_key),
            rows.contains(&PathBuf::new()),
        ),
        (Some(first_key.clone()), true, false),
    );
    let open = check.workspace(window, |w, cx| w.tree_expanded(cx));
    check.expect_eq(
        "the folder that was open stays open",
        open.is_some_and(|dirs| dirs.contains(&first_key)),
        true,
    );
    check.shot(window, "folders-1-two");

    check.keys(window, "cmd-shift-o q u e r y enter");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "the finder opens a file in the second folder",
        active,
        Some(second_key.join("queries/query.sql")),
    );
    check.keys(window, "cmd-alt-shift-c");
    check.expect_eq(
        "the relative path is inside its own folder",
        check.clipboard(),
        Some("queries/query.sql:1".to_string()),
    );
    check.keys(window, "cmd-shift-c");
    check.expect_eq(
        "the absolute path is on disk",
        check.clipboard(),
        Some(
            second_root
                .join("queries/query.sql")
                .to_string_lossy()
                .to_string(),
        ),
    );

    check.cx.advance_clock(Duration::from_secs(2));
    check.cx.run_until_parked();
    let saved = session::load(&session::default_path())
        .window(&first_root)
        .map(|w| w.folders.clone());
    check.expect_eq(
        "the session keeps the added folder",
        saved,
        Some(vec![second_root.clone()]),
    );

    // Right-click the second folder's row, then its Remove from Window.
    let rows = check.workspace(window, |w, cx| w.tree_rows(cx));
    let index = rows.iter().position(|r| *r == second_key).expect("row") as f32;
    let row_y = FIRST_ROW + ROW * index;
    let at = point(px(60.), px(row_y + ROW / 2.));
    check
        .cx
        .simulate_mouse_down(window, at, MouseButton::Right, Modifiers::default());
    check
        .cx
        .simulate_mouse_up(window, at, MouseButton::Right, Modifiers::default());
    check.cx.run_until_parked();
    check.shot(window, "folders-2-menu");
    // Remove from Window is the menu's last line; Up from the first goes round to it.
    check.keys(window, "up enter");
    let left = check.workspace(window, |w, cx| w.folders(cx).map(|f| f.roots().to_vec()));
    check.expect_eq(
        "remove from window leaves the first folder",
        left,
        Some(vec![first_root.clone()]),
    );
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq("its tab closed with it", active, None);
}
