<img src="assets/brand/mark.svg" width="72" alt="">

# leaf

A light, fast, keyboard-first text and Markdown editor for macOS, written in Rust on
[GPUI](https://github.com/zed-industries/zed) (through [GPUI Kit](https://github.com/longbridge/gpui-component)).

Open a folder, walk its tree with the arrow keys, copy paths in one keystroke, and read Markdown
and code with syntax highlighting, with your shell a keystroke away. An editor, not an IDE: no
language servers or plugins.

Early days: macOS only, built from source.

## Build and run

You need macOS and Rust (the toolchain in `rust-toolchain.toml` is picked up automatically).
The Xcode Command Line Tools are enough; shaders compile when the app starts.

```bash
cargo run --release -p leaf -- ~/notes          # open a folder
cargo run --release -p leaf -- ~/notes/todo.md  # open a file in its folder
```

Without an argument leaf reopens the windows you left, each with its folder, tree and tabs.

## Install

There is no download yet; build the app from source:

```bash
scripts/bundle.sh         # target/bundle/leaf.app
scripts/bundle.sh --dmg   # also target/bundle/leaf-<version>.dmg
```

Copy `target/bundle/leaf.app` to `/Applications` (or open the DMG and drag it there).
Then Finder's Open With and `open -a leaf <folder or file>` open in leaf, a new window per folder.
The app is signed ad hoc, which is enough on the Mac that built it; giving it to others would need
a Developer ID signature and notarization.

### The `leaf` command

`scripts/leaf` opens folders and files from a terminal, starting leaf or handing them to the one
already running:

```bash
leaf ~/notes            # the folder, in the window that has it or a new one
leaf ~/notes/todo.md    # the file, in its folder's window
leaf                    # start leaf, or bring it forward
```

To have it on your `PATH`, link it into a folder that is on `PATH`, from the repository root:

```bash
mkdir -p ~/.local/bin
ln -s "$PWD/scripts/leaf" ~/.local/bin/leaf
```

It opens `target/bundle/leaf.app` from this repository when that exists, else the `leaf.app`
macOS knows about (the one in `/Applications`). Set `LEAF_APP=/path/to/leaf.app` to choose another.

## Keys

The keys follow IntelliJ IDEA.

| Where | Keys | What |
|---|---|---|
| Anywhere | `Cmd+O` | Open a folder (in a new window when this one has a folder) |
| | `Cmd+Shift+N` | New window |
| | `Cmd+1` | Focus the tree (again: hide it) |
| | `Cmd+Shift+O` | Find a file by name |
| | `Cmd+Shift+C` | Copy the absolute path |
| | `Cmd+Alt+Shift+C` | Copy the relative path; in the editor with the line, `docs/a.md:12` or `:12-18` |
| | `Cmd+Alt+R` | Reveal in Finder |
| | `Cmd+S`, `Cmd+W` | Save, close the tab |
| | `Cmd+Shift+[` / `]` | Previous / next tab |
| | `Cmd+Shift+V` | Markdown preview |
| | `Cmd+=`, `Cmd+-`, `Cmd+0` | Zoom in, out, back to 100% |
| | `Alt+F12` | Terminal in the open folder (in the terminal: hide it, the shell keeps running) |
| Tree | `Up`, `Down`, `Home`, `End`, `PageUp`, `PageDown` | Move |
| | `Right` / `Left` | Open a folder or step in / close it or step out |
| | `Enter` | Open the file (or toggle the folder) |
| | `Space` | Leaf through: show the file and stay in the tree |
| | type letters | Jump to a name |
| | `Cmd+N`, `Shift+F6`, `Cmd+Backspace` | New file (`name/` for a folder), rename, move to the Trash |
| | `Esc` | Back to the editor |
| Terminal | `Cmd+C`, `Cmd+V` | Copy the selection, paste |
| Markdown | `Enter`, `Tab`, `Shift+Tab` | Continue a list, indent or outdent a list item |
| | `Cmd+B`, `Cmd+I` | Bold, italic |

## Settings

leaf writes `~/Library/Application Support/leaf/settings.json` with the defaults on first run.
Changes apply the next time leaf starts.

```json
{
  "editor_font": "SF Mono",
  "editor_font_size": 13.0,
  "tab_size": 4,
  "soft_wrap": "markdown",
  "key_hints": true,
  "keymap": [
    { "key": "cmd-p", "action": "workspace::ToggleFinder", "context": "Workspace" }
  ]
}
```

`soft_wrap` is `markdown`, `all` or `none`. Entries in `keymap` are added after leaf's own keys, so they win.
A file that does not parse is ignored (with the reason in the status bar) and the defaults are used.

## Development

```bash
cargo test --workspace   # unit tests, plus offscreen UI checks that save screenshots to target/visual/
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

The UI checks (`crates/app/tests/visual/`) build the real window off screen, type keys into it and
check what happens, without showing a window or taking the keyboard.

`crates/project` holds the rules that need no UI (the tree, keyboard moves, copied paths, file I/O,
session and settings); `crates/app` is the window.

## License

[MIT](LICENSE)
