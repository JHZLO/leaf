//! Every key binding in leaf, in one table (IntelliJ keys; the design doc's keymap section).

use gpui_kit::*;

/// Anywhere in the window.
pub const WORKSPACE: &str = "Workspace";
/// The folder tree has focus.
pub const PROJECT_PANEL: &str = "ProjectPanel";

actions!(
    workspace,
    [
        OpenFolder,
        FocusTree,
        CopyPath,
        CopyRelativePath,
        Save,
        CloseTab,
        PrevTab,
        NextTab,
        TogglePreview,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        Quit,
        ToggleFinder,
        RevealInFinder,
    ]
);

actions!(
    project_panel,
    [
        SelectPrev,
        SelectNext,
        Expand,
        Collapse,
        SelectFirst,
        SelectLast,
        PageUp,
        PageDown,
        Open,
        LeafThrough,
        FocusEditor,
        NewFile,
        Rename,
        Trash,
    ]
);

pub fn init(cx: &mut App) {
    let ws = Some(WORKSPACE);
    let panel = Some(PROJECT_PANEL);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-o", OpenFolder, ws),
        KeyBinding::new("cmd-1", FocusTree, ws),
        KeyBinding::new("cmd-shift-c", CopyPath, ws),
        KeyBinding::new("cmd-alt-shift-c", CopyRelativePath, ws),
        KeyBinding::new("cmd-s", Save, ws),
        KeyBinding::new("cmd-w", CloseTab, ws),
        KeyBinding::new("cmd-shift-[", PrevTab, ws),
        KeyBinding::new("cmd-shift-]", NextTab, ws),
        KeyBinding::new("cmd-shift-v", TogglePreview, ws),
        KeyBinding::new("cmd-=", ZoomIn, ws),
        KeyBinding::new("cmd-+", ZoomIn, ws),
        KeyBinding::new("cmd--", ZoomOut, ws),
        KeyBinding::new("cmd-0", ZoomReset, ws),
        KeyBinding::new("up", SelectPrev, panel),
        KeyBinding::new("down", SelectNext, panel),
        KeyBinding::new("right", Expand, panel),
        KeyBinding::new("left", Collapse, panel),
        KeyBinding::new("home", SelectFirst, panel),
        KeyBinding::new("end", SelectLast, panel),
        KeyBinding::new("pageup", PageUp, panel),
        KeyBinding::new("pagedown", PageDown, panel),
        KeyBinding::new("enter", Open, panel),
        KeyBinding::new("space", LeafThrough, panel),
        KeyBinding::new("escape", FocusEditor, panel),
        KeyBinding::new("cmd-shift-o", ToggleFinder, ws),
        KeyBinding::new("cmd-alt-r", RevealInFinder, ws),
        KeyBinding::new("cmd-n", NewFile, panel),
        KeyBinding::new("shift-f6", Rename, panel),
        KeyBinding::new("cmd-backspace", Trash, panel),
        // The recent folders on the empty window.
        KeyBinding::new("up", SelectPrev, Some("Workspace && empty")),
        KeyBinding::new("down", SelectNext, Some("Workspace && empty")),
        KeyBinding::new("enter", Open, Some("Workspace && empty")),
        // Markdown editing keys (Phase 2) go below this line, in their own block.
    ]);
}
