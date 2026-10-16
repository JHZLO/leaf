//! Tree speed search, file operations, following the open file.

use std::path::{Path, PathBuf};

use gpui_kit::{
    AnyWindowHandle,
    component::{Theme, ThemeMode},
};

use crate::check::{Check, fixture};

pub fn run(check: &mut Check) {
    let was = check.cx.update(|cx| Theme::global(cx).mode);
    speed_search(check);
    new_file(check);
    check.cx.update(|cx| Theme::change(was, None, cx));
}

/// A window on `root` in the light theme. Set after opening: a new window takes the system's.
fn open_light(check: &mut Check, root: &Path) -> AnyWindowHandle {
    let window = check.open(Some(root));
    check
        .cx
        .update(|cx| Theme::change(ThemeMode::Light, None, cx));
    window
}

fn selection(check: &mut Check, window: AnyWindowHandle) -> Option<PathBuf> {
    check.workspace(window, |w, cx| w.tree_selection(cx))
}

/// Typing a name selects it; Up and Down walk the matches; Escape clears; Enter opens.
fn speed_search(check: &mut Check) {
    let fixture = fixture();
    let window = open_light(check, fixture.path());
    // Open docs and docs/guide; the guide folder stays selected.
    check.keys(window, "down right down right");
    check.keys(window, ". m d");
    let selected = selection(check, window);
    check.expect_eq(
        "typing .md selects the first match below the selection",
        selected,
        Some(PathBuf::from("docs/guide/keys.md")),
    );
    check.keys(window, "down down up");
    let selected = selection(check, window);
    check.expect_eq(
        "down and up walk the matches",
        selected,
        Some(PathBuf::from("docs/guide/paths.md")),
    );
    check.shot(window, "tree-1-speed-search");

    // The first Escape only clears the search: the tree keeps focus, so the copy is the tree's.
    check.keys(window, "escape cmd-alt-shift-c");
    let copied = check.clipboard();
    check.expect_eq(
        "escape clears the search and keeps the tree focused",
        copied,
        Some("docs/guide/paths.md".into()),
    );

    // A new search starts from the selection and wraps past the last row.
    check.keys(window, "k e y");
    let selected = selection(check, window);
    check.expect_eq(
        "typing key wraps around to keys.md",
        selected,
        Some(PathBuf::from("docs/guide/keys.md")),
    );

    // Enter opens the match.
    check.keys(window, "escape r e a");
    let selected = selection(check, window);
    check.expect_eq(
        "typing rea selects README.md",
        selected,
        Some(PathBuf::from("README.md")),
    );
    check.keys(window, "enter");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "enter opens the match",
        active,
        Some(PathBuf::from("README.md")),
    );
}

/// Cmd+N types a name under the selected folder; slashes make the folders on the way.
fn new_file(check: &mut Check) {
    let fixture = fixture();
    let window = open_light(check, fixture.path());
    check.keys(window, "down cmd-n d r a f t s / i d e a . m d");
    check.shot(window, "tree-2-new-file");
    check.keys(window, "enter");
    let created = fixture.path().join("docs/drafts/idea.md");
    check.expect_eq("cmd-n creates docs/drafts/idea.md", created.is_file(), true);
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "the new file is the active tab",
        active,
        Some(PathBuf::from("docs/drafts/idea.md")),
    );
    let selected = selection(check, window);
    check.expect_eq(
        "the new file is selected in the tree",
        selected,
        Some(PathBuf::from("docs/drafts/idea.md")),
    );

    // The same name again is refused and the input stays, with the reason after it.
    std::fs::write(&created, "kept\n").expect("write");
    check.keys(window, "cmd-1 cmd-n i d e a . m d enter");
    check.shot(window, "tree-3-new-file-exists");
    let kept = std::fs::read_to_string(&created).ok();
    check.expect_eq(
        "an existing file is not replaced",
        kept,
        Some("kept\n".into()),
    );

    // Escape cancels; the tree keeps focus, so the copy is the tree's selection.
    check.keys(window, "escape cmd-alt-shift-c");
    let copied = check.clipboard();
    check.expect_eq(
        "escape cancels the new name and keeps the tree focused",
        copied,
        Some("docs/drafts/idea.md".into()),
    );
}
