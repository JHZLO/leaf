//! Several windows, each with its own folder and session.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use gpui_kit::AnyWindowHandle;
use leaf::workspace::{self, OffscreenWindows};
use leaf_project::session;

use crate::check::{Check, fixture};

pub fn run(check: &mut Check) {
    // Windows leaf opens by itself (not through Check::open) stay off screen as well.
    check.cx.update(|cx| cx.set_global(OffscreenWindows));

    let notes = fixture();
    let code = fixture();
    let notes_root = notes.path().canonicalize().expect("root");
    let code_root = code.path().canonicalize().expect("root");

    // A folder from outside (the command line, Finder) gets its own window.
    let first = check.open(Some(&notes_root));
    let before = windows(check);
    let second = check
        .cx
        .update(|cx| workspace::open_path(&code_root, cx))
        .expect("a window");
    check.cx.run_until_parked();
    let count = windows(check).len();
    check.expect_eq(
        "a second folder opens a new window",
        count,
        before.len() + 1,
    );
    let root = check.workspace(second, |w, cx| w.root(cx));
    check.expect_eq(
        "the new window shows that folder",
        root,
        Some(code_root.clone()),
    );

    // The same folder again brings its window forward instead of opening another.
    let used = check.cx.update(|cx| workspace::open_path(&notes_root, cx));
    check.expect_eq("an open folder comes forward", used, Some(first));
    let count = windows(check).len();
    check.expect_eq("and no window is added", count, before.len() + 1);

    // A file opens in the window whose folder holds it.
    let used = check
        .cx
        .update(|cx| workspace::open_path(&notes_root.join("docs/notes.md"), cx));
    check.cx.run_until_parked();
    check.expect_eq("a file goes to its folder's window", used, Some(first));
    let active = check.workspace(first, |w, cx| w.active_file(cx));
    check.expect_eq(
        "and opens there",
        active,
        Some(PathBuf::from("docs/notes.md")),
    );

    // Cmd+Shift+N opens an empty window.
    let before = windows(check);
    check.keys(first, "cmd-shift-n");
    let added = new_windows(check, &before);
    check.expect_eq("cmd-shift-n opens one window", added.len(), 1);
    let Some(&empty) = added.first() else {
        return;
    };
    let root = check.workspace(empty, |w, cx| w.root(cx));
    check.expect_eq("the new window is empty", root, None);
    check.shot(empty, "windows-1-new-window");

    // Cmd+O's folder: an empty window takes it; a window with a folder keeps its own.
    let other = fixture();
    let other_root = other.path().canonicalize().expect("root");
    let used = check
        .cx
        .update(|cx| workspace::open_folder_from(other_root.clone(), empty, cx));
    check.cx.run_until_parked();
    check.expect_eq("an empty window takes the chosen folder", used, Some(empty));
    let root = check.workspace(empty, |w, cx| w.root(cx));
    check.expect_eq("and shows it", root, Some(other_root));
    let another = fixture();
    let another_root = another.path().canonicalize().expect("root");
    let before = windows(check);
    let used = check
        .cx
        .update(|cx| workspace::open_folder_from(another_root.clone(), first, cx));
    check.cx.run_until_parked();
    let added = new_windows(check, &before);
    check.expect_eq(
        "a window with a folder opens the chosen one in a new window",
        added,
        used.into_iter().collect::<Vec<_>>(),
    );
    let root = check.workspace(first, |w, cx| w.root(cx));
    check.expect_eq("and keeps its own folder", root, Some(notes_root.clone()));

    // Both windows reach the session file, each with its own tree and tabs.
    check.keys(second, "down down right");
    check
        .cx
        .update(|cx| workspace::open_path(&code_root.join("src/main.rs"), cx));
    check.cx.run_until_parked();
    check.cx.advance_clock(Duration::from_secs(2));
    check.cx.run_until_parked();
    let saved = session::load(&session::default_path());
    let notes_tabs = tab_paths(saved.window(&notes_root));
    check.expect_eq(
        "the session keeps the first window's tabs",
        notes_tabs,
        vec![PathBuf::from("docs/notes.md")],
    );
    let code_window = saved.window(&code_root);
    check.expect_eq(
        "the session keeps the second window's tabs",
        tab_paths(code_window),
        vec![PathBuf::from("src/main.rs")],
    );
    check.expect_eq(
        "and its open folders",
        code_window.map(|w| w.expanded.clone()),
        // docs opened by hand, src by the tree following src/main.rs.
        Some(vec![PathBuf::from("docs"), PathBuf::from("src")]),
    );
    check.expect_eq(
        "the recent folders are shared",
        saved.recent.iter().take(4).cloned().collect::<Vec<_>>(),
        vec![
            another.path().canonicalize().expect("root"),
            other.path().canonicalize().expect("root"),
            code_root.clone(),
            notes_root.clone(),
        ],
    );

    // Quit and start again: both windows come back from the file as they were.
    let file = std::fs::read(session::default_path()).expect("session file");
    for window in [first, second] {
        close(check, window);
    }
    std::fs::write(session::default_path(), file).expect("session file");
    check
        .cx
        .update(|cx| workspace::open_at_start(Vec::new(), cx));
    check.cx.run_until_parked();
    // An empty window left open would take the first folder, so look through every window.
    let all = windows(check);
    let notes_again = window_on(check, &all, &notes_root);
    let code_again = window_on(check, &all, &code_root);
    check.expect_eq(
        "both windows reopen at start",
        (notes_again.is_some(), code_again.is_some()),
        (true, true),
    );
    if let (Some(notes_again), Some(code_again)) = (notes_again, code_again) {
        let active = check.workspace(notes_again, |w, cx| w.active_file(cx));
        check.expect_eq(
            "the first window's tab comes back",
            active,
            Some(PathBuf::from("docs/notes.md")),
        );
        let active = check.workspace(code_again, |w, cx| w.active_file(cx));
        check.expect_eq(
            "the second window's tab comes back",
            active,
            Some(PathBuf::from("src/main.rs")),
        );
        check.shot(code_again, "windows-2-restored");
    }
}

fn windows(check: &mut Check) -> Vec<AnyWindowHandle> {
    check.cx.update(|cx| cx.windows())
}

fn new_windows(check: &mut Check, before: &[AnyWindowHandle]) -> Vec<AnyWindowHandle> {
    windows(check)
        .into_iter()
        .filter(|w| !before.contains(w))
        .collect()
}

/// Closes a window as its close button does, and tells leaf, as the app's close handler does.
fn close(check: &mut Check, window: AnyWindowHandle) {
    check
        .cx
        .update_window(window, |_, window, _| window.remove_window())
        .expect("window");
    check
        .cx
        .update(|cx| workspace::window_closed(window.window_id(), cx));
    check.cx.run_until_parked();
}

fn window_on(check: &mut Check, among: &[AnyWindowHandle], root: &Path) -> Option<AnyWindowHandle> {
    among
        .iter()
        .copied()
        .find(|&w| check.workspace(w, |w, cx| w.root(cx)).as_deref() == Some(root))
}

fn tab_paths(window: Option<&session::WindowState>) -> Vec<PathBuf> {
    window
        .map(|w| w.tabs.iter().map(|t| t.path.clone()).collect())
        .unwrap_or_default()
}
