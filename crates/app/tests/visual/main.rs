//! Offscreen UI checks. Builds the real leaf window off screen, drives it with keystrokes, checks the
//! results and saves screenshots to target/visual/. It never shows a window, never takes the keyboard
//! and never touches the real session. Run with `cargo test -p leaf --test visual`.
//! Each feature keeps its scenarios in its own module.

mod check;
mod finder;
mod markdown;
mod phase1;
mod tree;

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
    }

    let mut cx = VisualTestAppContext::new(current_platform(false));
    cx.update(leaf::init);
    let mut check = check::Check {
        cx,
        out,
        failures: Vec::new(),
    };

    phase1::run(&mut check);
    finder::run(&mut check);
    tree::run(&mut check);
    markdown::run(&mut check);

    if check.failures.is_empty() {
        println!("visual checks passed; screenshots in target/visual/");
    } else {
        for failure in &check.failures {
            eprintln!("FAIL {failure}");
        }
        std::process::exit(1);
    }
}
