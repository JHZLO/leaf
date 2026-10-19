//! The terminal: open with the folder, Alt+F12 gives it focus, typed text and keys reach the shell,
//! Alt+F12 in it hides it (the shell lives on), and `exit` closes it. The shell is /bin/sh (`LEAF_SHELL`), so
//! nobody's login files change what the checks see.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use gpui_kit::AnyWindowHandle;

use crate::check::{Check, fixture};

pub fn run(check: &mut Check) {
    let fixture = fixture();
    let root = fixture.path().canonicalize().expect("root");
    let window = check.open(Some(&root));
    let opened = check.workspace(window, |w, cx| w.terminal_lines(cx).is_some());
    check.expect_eq("the terminal is open with the folder", opened, true);
    // Focus stays in the tree: Down there selects the open folder's row.
    check.keys(window, "down");
    let selected = check.workspace(window, |w, cx| w.tree_selection(cx));
    check.expect_eq(
        "the tree keeps the focus",
        selected,
        Some(std::path::PathBuf::new()),
    );
    check.keys(window, "alt-f12");

    // Type once the prompt is up, as a person would.
    let prompt = wait_for(check, window, |lines| {
        lines.iter().any(|l| l.ends_with('$'))
    });
    check.expect_eq("the shell shows its prompt", prompt, true);
    check.keys(window, "e c h o space h i enter");
    let echoed = wait_for(check, window, |lines| lines.iter().any(|l| l == "hi"));
    check.expect_eq("typed text and enter reach the shell", echoed, true);

    check.keys(window, "p w d enter");
    let here = wait_for(check, window, |lines| {
        lines.iter().any(|l| Path::new(l) == root)
    });
    check.expect_eq("the shell starts in the open folder", here, true);

    // Ctrl+C interrupts a running command; the next one runs at once.
    check.keys(
        window,
        "s l e e p space 3 0 enter ctrl-c e c h o space o k enter",
    );
    let interrupted = wait_for(check, window, |lines| lines.iter().any(|l| l == "ok"));
    check.expect_eq("ctrl-c interrupts sleep", interrupted, true);
    check.shot(window, "terminal-1-shell");

    // Tab belongs to the shell, not to focus moves: the terminal keeps the keys after it.
    check.keys(window, "tab e c h o space t a b b e d enter");
    let tabbed = wait_for(check, window, |lines| {
        lines.iter().any(|l| l.ends_with("tabbed"))
    });
    check.expect_eq("tab stays in the terminal", tabbed, true);

    check.keys(window, "alt-f12");
    let hidden = check.workspace(window, |w, cx| w.terminal_lines(cx).is_none());
    check.expect_eq("alt-f12 in the terminal hides it", hidden, true);
    check.keys(window, "alt-f12");
    let kept = check.workspace(window, |w, cx| {
        w.terminal_lines(cx)
            .is_some_and(|lines| lines.iter().any(|l| l == "hi"))
    });
    check.expect_eq("the hidden shell lives on", kept, true);

    // Cmd+T: a second shell in its own tab, in front.
    check.keys(window, "cmd-t");
    let tabs = check.workspace(window, |w, cx| w.terminal_tabs(cx));
    check.expect_eq("cmd-t opens a second terminal tab", tabs, 2);
    let fresh = wait_for(check, window, |lines| {
        lines.iter().any(|l| l.ends_with('$')) && !lines.iter().any(|l| l == "hi")
    });
    check.expect_eq("the new tab is a fresh shell", fresh, true);
    check.keys(window, "e c h o space t w o enter");
    let two = wait_for(check, window, |lines| lines.iter().any(|l| l == "two"));
    check.expect_eq("keys go to the tab in front", two, true);
    check.shot(window, "terminal-2-tabs");
    check.keys(window, "cmd-shift-[");
    let first = wait_for(check, window, |lines| lines.iter().any(|l| l == "hi"));
    check.expect_eq("cmd-shift-[ goes back to the first tab", first, true);
    check.keys(window, "cmd-w");
    let tabs = check.workspace(window, |w, cx| w.terminal_tabs(cx));
    check.expect_eq("cmd-w closes the tab in front", tabs, 1);
    let left = wait_for(check, window, |lines| lines.iter().any(|l| l == "two"));
    check.expect_eq("the other tab comes to the front", left, true);

    check.keys(window, "e x i t enter");
    let closed = wait_for_workspace(check, window, |w, cx| w.terminal_lines(cx).is_none());
    check.expect_eq("exit in the last tab closes the terminal", closed, true);
}

/// The shell answers on its own thread; give it up to five seconds.
fn wait_for(check: &mut Check, window: AnyWindowHandle, found: impl Fn(&[String]) -> bool) -> bool {
    wait_for_workspace(check, window, |w, cx| {
        w.terminal_lines(cx).is_some_and(|lines| {
            let lines: Vec<String> = lines.iter().map(|l| l.trim_end().to_string()).collect();
            found(&lines)
        })
    })
}

fn wait_for_workspace(
    check: &mut Check,
    window: AnyWindowHandle,
    found: impl Fn(&leaf::workspace::Workspace, &gpui_kit::App) -> bool,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        check.cx.run_until_parked();
        if check.workspace(window, &found) {
            return true;
        }
        if Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}
