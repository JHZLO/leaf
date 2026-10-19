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
    new_directory(check);
    root_row(check);
    header(check);
    rename(check);
    tabs_follow(check);
    check.cx.update(|cx| Theme::change(was, None, cx));
}

/// Open tabs follow a rename in the tree, and the tree follows the file a tab switch shows.
fn tabs_follow(check: &mut Check) {
    let fixture = fixture();
    let window = open_light(check, fixture.path());
    // Open docs/notes.md, then rename it from the tree.
    check.keys(window, "down down right down down enter");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "enter opens docs/notes.md",
        active,
        Some(PathBuf::from("docs/notes.md")),
    );
    check.keys(window, "cmd-1 shift-f6 cmd-a j o u r n a l . m d enter");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "the open tab follows a rename",
        active,
        Some(PathBuf::from("docs/journal.md")),
    );
    // Open a nested file from the finder; the tree opens its folders and selects it.
    check.keys(window, "cmd-1 left left up cmd-shift-o g p a t h enter");
    let selected = selection(check, window);
    check.expect_eq(
        "the tree follows the active file",
        selected,
        Some(PathBuf::from("docs/guide/paths.md")),
    );
    check.shot(window, "tree-6-follows-active-file");
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
    check.keys(window, "down down right down right");
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
    // Cmd+N asks first: File (Enter, the first line) or Directory.
    check.keys(window, "down down cmd-n");
    check.shot(window, "tree-2-new-popup");
    check.keys(window, "enter d r a f t s / i d e a . m d");
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
    check.keys(window, "cmd-1 cmd-n f i d e a . m d enter");
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

/// The New popup's Directory makes a folder from the name, without a trailing slash; Escape
/// closes the popup without making anything.
fn new_directory(check: &mut Check) {
    let fixture = fixture();
    let window = open_light(check, fixture.path());
    check.keys(window, "down cmd-n escape");
    let before = std::fs::read_dir(fixture.path()).expect("read").count();
    check.keys(window, "cmd-n down enter n o t e s enter");
    check.expect_eq(
        "directory makes a folder named as typed",
        fixture.path().join("notes").is_dir(),
        true,
    );
    check.expect_eq(
        "escape made nothing",
        std::fs::read_dir(fixture.path()).expect("read").count(),
        before + 1,
    );
    // D picks Directory at once; the new folder is selected, so this one goes inside it.
    check.keys(window, "cmd-n d d r a f t s enter");
    check.expect_eq(
        "d picks directory",
        fixture.path().join("notes/drafts").is_dir(),
        true,
    );
}

/// The header's buttons: Expand All opens every folder but gitignored ones, Collapse All closes
/// them, the minus hides the tree.
fn header(check: &mut Check) {
    let fixture = fixture();
    let window = open_light(check, fixture.path());
    check.shot(window, "tree-8-header");
    // The buttons sit right of "Project", 22 wide, ending 6 in from the tree's right edge.
    let y = 38. + 17.;
    let button = |from_right: f32| 6. + 300. - 6. - 11. - 24. * from_right;
    click(check, window, button(3.), y);
    let expanded = wait_expanded(check, window, |dirs| {
        dirs.iter().any(|d| d.ends_with("guide"))
    });
    check.expect_eq("expand all opens nested folders", expanded, true);
    let ignored = check.workspace(window, |w, cx| {
        w.tree_expanded(cx)
            .is_some_and(|dirs| dirs.iter().any(|d| d.ends_with("target")))
    });
    check.expect_eq("expand all leaves gitignored folders shut", ignored, false);
    click(check, window, button(2.), y);
    let collapsed = check.workspace(window, |w, cx| w.tree_expanded(cx));
    check.expect_eq("collapse all closes them", collapsed, Some(Vec::new()));
    click(check, window, button(0.), y);
    let hidden = check.workspace(window, |w, cx| w.tree_expanded(cx));
    check.expect_eq("the minus hides the tree", hidden, None);
}

fn click(check: &mut Check, window: AnyWindowHandle, x: f32, y: f32) {
    let at = gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
    check
        .cx
        .simulate_click(window, at, gpui_kit::Modifiers::default());
    check.cx.run_until_parked();
}

/// Expand All reads folders off the main thread; give it a moment.
fn wait_expanded(
    check: &mut Check,
    window: AnyWindowHandle,
    found: impl Fn(&[PathBuf]) -> bool,
) -> bool {
    for _ in 0..100 {
        check.cx.run_until_parked();
        let done = check.workspace(window, |w, cx| {
            w.tree_expanded(cx).is_some_and(|dirs| found(&dirs))
        });
        if done {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

/// The open folder is the first row (IntelliJ): it can be selected, Cmd+N there makes a file at
/// the top, and Cmd+Shift+C copies the folder's own path.
fn root_row(check: &mut Check) {
    let fixture = fixture();
    let window = open_light(check, fixture.path());
    check.keys(window, "down");
    let selected = selection(check, window);
    check.expect_eq(
        "down from nothing selects the open folder",
        selected,
        Some(PathBuf::new()),
    );
    check.shot(window, "tree-7-root-row");
    check.keys(window, "cmd-n f t o d o . m d enter");
    check.expect_eq(
        "cmd-n on the open folder creates todo.md at the top",
        fixture.path().join("todo.md").is_file(),
        true,
    );
    check.keys(window, "cmd-1 home cmd-shift-c");
    let copied = check.clipboard();
    let root = fixture.path().canonicalize().expect("root");
    check.expect_eq(
        "cmd-shift-c on the open folder copies its path",
        copied,
        Some(root.to_string_lossy().to_string()),
    );
}

/// Shift+F6 turns the name into an input with the part before the extension selected.
fn rename(check: &mut Check) {
    let fixture = fixture();
    let window = open_light(check, fixture.path());
    // README.md is the last row.
    check.keys(window, "end shift-f6 i n d e x");
    check.shot(window, "tree-4-rename");
    check.keys(window, "enter");
    let renamed = (
        fixture.path().join("index.md").is_file(),
        fixture.path().join("README.md").exists(),
    );
    check.expect_eq(
        "shift-f6 renames README.md to index.md on disk",
        renamed,
        (true, false),
    );
    let selected = selection(check, window);
    check.expect_eq(
        "the renamed file stays selected",
        selected,
        Some(PathBuf::from("index.md")),
    );

    // A name another item has is refused and nothing moves.
    check.keys(window, "shift-f6 cmd-a . g i t i g n o r e enter");
    check.shot(window, "tree-5-rename-exists");
    check.keys(window, "escape");
    let names = (
        fixture.path().join("index.md").is_file(),
        std::fs::read_to_string(fixture.path().join(".gitignore")).ok(),
    );
    check.expect_eq(
        "a rename onto .gitignore is refused",
        names,
        (true, Some("target/\n".into())),
    );
}
