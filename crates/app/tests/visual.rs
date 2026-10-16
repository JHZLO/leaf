//! Offscreen UI checks. Builds the real leaf window off screen, drives it with keystrokes, checks the
//! results and saves screenshots to target/visual/. It never shows a window, never takes the keyboard
//! and never touches the real session. Run with `cargo test -p leaf --test visual`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use gpui_kit::{
    component::{Theme, ThemeMode},
    platform::current_platform,
    test::TestWindowExt as _,
    *,
};
use leaf::workspace::Workspace;

const FILES: &[(&str, &str)] = &[
    (
        "docs/guide/keys.md",
        "# Keys\n\nArrow keys walk the tree.\n",
    ),
    (
        "docs/guide/paths.md",
        "# Copying paths\n\nleaf copies a path in one keystroke,\nfrom the tree or from the editor.\n\n\
         | Keys | You get |\n|---|---|\n| Cmd+Shift+C | the absolute path |\n| Cmd+Alt+Shift+C | docs/guide/paths.md:12 |\n\n\
         ## Why the line number\n\nPaste it into a PR or a chat and the reader lands on `docs/guide/paths.md:12`.\n\n\
         - Folders copy without a trailing slash\n- 한글 파일명은 NFC 로 복사된다\n- [ ] Same keys in the tree and the editor\n\n\
         > Copying never needs the mouse.\n",
    ),
    ("docs/notes.md", "notes\n"),
    (
        "src/main.rs",
        "use std::path::Path;\n\n// Prints where leaf runs.\nfn main() {\n    let here = Path::new(\".\");\n    println!(\"leaf in {}\", here.display());\n}\n",
    ),
    ("README.md", "# leaf\n"),
    ("target/debug/leaf", "binary\n"),
    (".gitignore", "target/\n"),
];

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    for (path, text) in FILES {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, text).expect("write");
    }
    dir
}

struct Check {
    cx: VisualTestAppContext,
    out: PathBuf,
    failures: Vec<String>,
}

impl Check {
    fn open(&mut self, root: Option<&Path>) -> AnyWindowHandle {
        let root = root.map(|r| r.canonicalize().expect("root"));
        let window = self
            .cx
            .open_offscreen_window_default(move |window, cx| {
                let workspace = cx.new(|cx| {
                    let mut workspace = Workspace::new(window, cx);
                    if let Some(root) = root {
                        workspace.set_root(root, window, cx);
                    }
                    workspace
                });
                cx.new(|cx| gpui_kit::base::Root::new(workspace, window, cx))
            })
            .expect("offscreen window");
        self.cx.run_until_parked();
        window.into()
    }

    /// One key at a time, letting folder reads and file opens finish in between, as a person types.
    fn keys(&mut self, window: AnyWindowHandle, keys: &str) {
        for key in keys.split_whitespace() {
            self.cx.simulate_keystrokes(window, key);
            self.cx.run_until_parked();
        }
    }

    fn shot(&mut self, window: AnyWindowHandle, name: &str) {
        self.cx.run_until_parked();
        // An offscreen window draws only on input; a theme or zoom change needs a fresh frame.
        self.cx
            .update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        let path = self.out.join(format!("{name}.png"));
        match self.cx.capture_screenshot(window) {
            Ok(image) => {
                image.save(&path).expect("save png");
                println!("screenshot {}", path.display());
            }
            Err(err) => self.failures.push(format!("{name}: no screenshot ({err})")),
        }
    }

    fn workspace<R>(
        &mut self,
        window: AnyWindowHandle,
        read: impl FnOnce(&Workspace, &App) -> R,
    ) -> R {
        self.cx
            .update_window(window, |root, _, cx| {
                let root = root.downcast::<gpui_kit::base::Root>().expect("root view");
                let workspace = root
                    .read(cx)
                    .view()
                    .clone()
                    .downcast::<Workspace>()
                    .expect("workspace");
                read(workspace.read(cx), cx)
            })
            .expect("window")
    }

    fn expect_eq<T: PartialEq + std::fmt::Debug>(&mut self, what: &str, got: T, want: T) {
        if got == want {
            println!("ok   {what}");
        } else {
            self.failures
                .push(format!("{what}: got {got:?}, want {want:?}"));
        }
    }

    fn clipboard(&self) -> Option<String> {
        self.cx.read_from_clipboard().and_then(|item| item.text())
    }
}

fn main() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/visual");
    fs::create_dir_all(&out).expect("out dir");
    let session = tempfile::NamedTempFile::new().expect("session file");
    // Safe: set before anything else runs, on the only thread.
    unsafe { std::env::set_var("LEAF_SESSION_PATH", session.path()) };

    let mut cx = VisualTestAppContext::new(current_platform(false));
    cx.update(leaf::init);
    let mut check = Check {
        cx,
        out,
        failures: Vec::new(),
    };
    let fixture = fixture();

    // The tree: walk into docs/guide with the arrow keys; the vein follows the selection.
    let window = check.open(Some(fixture.path()));
    check.keys(window, "down right down right down down");
    let selected = check.workspace(window, |w, cx| w.tree_selection(cx));
    check.expect_eq(
        "arrow keys select docs/guide/paths.md",
        selected,
        Some(PathBuf::from("docs/guide/paths.md")),
    );
    check.keys(window, "cmd-alt-shift-c");
    let copied = check.clipboard();
    check.expect_eq(
        "relative path from the tree",
        copied,
        Some("docs/guide/paths.md".into()),
    );
    check.shot(window, "1-tree-vein-copied");

    // Space opens it as the leaf-through tab and keeps focus in the tree.
    check.keys(window, "space");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "space opens the selection",
        active,
        Some(PathBuf::from("docs/guide/paths.md")),
    );
    check.shot(window, "2-leaf-through");

    // Escape moves to the editor; the copy now carries the cursor line.
    check.keys(window, "escape cmd-alt-shift-c");
    let copied = check.clipboard();
    check.expect_eq(
        "relative path and line from the editor",
        copied,
        Some("docs/guide/paths.md:1".into()),
    );

    check.keys(window, "cmd-shift-v");
    check.shot(window, "3-preview");

    check
        .cx
        .update(|cx| Theme::change(ThemeMode::Light, None, cx));
    check.shot(window, "4-preview-light");
    check.keys(window, "cmd-shift-v");

    // A code file opens full width.
    check.keys(window, "cmd-1 down down right down enter");
    let active = check.workspace(window, |w, cx| w.active_file(cx));
    check.expect_eq(
        "enter opens src/main.rs",
        active,
        Some(PathBuf::from("src/main.rs")),
    );
    check.keys(window, "cmd-=");
    check.shot(window, "5-code-zoomed-light");
    check.keys(window, "cmd-0");

    check
        .cx
        .update(|cx| Theme::change(ThemeMode::Dark, None, cx));
    let empty = check.open(None);
    check.shot(empty, "6-empty-dark");

    if check.failures.is_empty() {
        println!("visual checks passed; screenshots in target/visual/");
    } else {
        for failure in &check.failures {
            eprintln!("FAIL {failure}");
        }
        std::process::exit(1);
    }
}
