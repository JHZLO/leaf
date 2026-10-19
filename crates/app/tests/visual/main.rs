//! Offscreen UI checks. Builds the real leaf window off screen, drives it with keystrokes, checks the
//! results and saves screenshots to target/visual/. It never shows a window, never takes the keyboard
//! and never touches the real session. Run with `cargo test -p leaf --test visual`.
//! Each feature keeps its scenarios in its own module.

mod check;
mod finder;
mod folders;
mod islands;
mod markdown;
mod markdown_view;
mod phase1;
mod preview_sync;
mod search;
mod terminal;
mod tree;
mod windows;

use std::{fs, path::PathBuf};

use gpui_kit::{platform::current_platform, *};

fn main() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/visual");
    fs::create_dir_all(&out).expect("out dir");
    let session = tempfile::NamedTempFile::new().expect("session file");
    // Settings the tests rely on: the defaults plus one extra key, to prove user bindings apply.
    let settings = tempfile::NamedTempFile::new().expect("settings file");
    fs::write(
        settings.path(),
        r#"{ "keymap": [ { "key": "cmd-p", "action": "workspace::ToggleFinder", "context": "Workspace" } ] }"#,
    )
    .expect("write settings");
    // Safe: set before anything else runs, on the only thread.
    unsafe {
        std::env::set_var("LEAF_SESSION_PATH", session.path());
        std::env::set_var("LEAF_SETTINGS_PATH", settings.path());
        std::env::set_var("LEAF_SHELL", "/bin/sh");
    }

    // The real assets, so icons show in the screenshots.
    let mut cx = VisualTestAppContext::with_asset_source(
        current_platform(false),
        std::sync::Arc::new(leaf::icons::LeafAssets),
    );
    cx.update(leaf::init);
    // An offscreen window draws only on input, so the tree's route is shot where it settles.
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut check = check::Check {
        cx,
        out,
        failures: Vec::new(),
    };

    phase1::run(&mut check);
    finder::run(&mut check);
    tree::run(&mut check);
    markdown::run(&mut check);
    search::run(&mut check);
    preview_sync::run(&mut check);
    windows::run(&mut check);
    terminal::run(&mut check);
    islands::run(&mut check);
    markdown_view::run(&mut check);
    folders::run(&mut check);

    if check.failures.is_empty() {
        println!("visual checks passed; screenshots in target/visual/");
    } else {
        for failure in &check.failures {
            eprintln!("FAIL {failure}");
        }
        std::process::exit(1);
    }
}
