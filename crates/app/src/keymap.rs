//! Every key binding in subwy, in one table (IntelliJ keys; the design doc's keymap section).

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
        CopyFileName,
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
        ToggleSearch,
        NewWindow,
        ToggleTerminal,
        OpenSettings,
    ]
);

actions!(
    project_panel,
    [
        SelectPrev,
        SelectNext,
        ExtendPrev,
        ExtendNext,
        Expand,
        Collapse,
        SelectFirst,
        SelectLast,
        PageUp,
        PageDown,
        Open,
        PassThrough,
        SpeedSearch,
        FocusEditor,
        NewFile,
        Rename,
        Trash,
        CutItems,
        CopyItems,
        PasteItems,
        CopyTo,
        MoveTo,
        OpenInTerminal,
        ExpandAll,
        CollapseAll,
        UndoChange,
    ]
);

/// The terminal panel has focus.
pub const TERMINAL: &str = "Terminal";

actions!(
    terminal,
    [
        CopySelection,
        Paste,
        SendTab,
        SendBackTab,
        NewTerminal,
        CloseTerminal,
        PrevTerminal,
        NextTerminal,
    ]
);

/// Around the editor of a Markdown tab.
pub const MARKDOWN: &str = "Markdown";
/// gpui-kit's editor ("Input") inside a Markdown tab.
const MARKDOWN_EDITOR: &str = "Markdown > Input";

actions!(
    markdown,
    [Newline, IndentItem, OutdentItem, ToggleBold, ToggleItalic]
);

/// Binds the user's extra keys from the settings file. Returns the first one that could not be bound.
pub fn bind_overrides(
    overrides: &[subwy_project::settings::KeyOverride],
    cx: &mut App,
) -> Option<String> {
    let mut problem = None;
    let mut bindings = Vec::new();
    for entry in overrides {
        let binding = cx
            .build_action(&entry.action, None)
            .map_err(|err| format!("unknown action {}: {err}", entry.action))
            .and_then(|action| {
                let context = match &entry.context {
                    Some(source) => Some(std::rc::Rc::new(
                        KeyBindingContextPredicate::parse(source)
                            .map_err(|err| format!("context {source}: {err}"))?,
                    )),
                    None => None,
                };
                KeyBinding::load(
                    &entry.key,
                    action,
                    context,
                    false,
                    None,
                    cx.keyboard_mapper().as_ref(),
                )
                .map_err(|err| format!("key {}: {err}", entry.key))
            });
        match binding {
            Ok(binding) => bindings.push(binding),
            Err(err) => {
                problem.get_or_insert(format!("settings.json keymap: {err}"));
            }
        }
    }
    cx.bind_keys(bindings);
    problem
}

pub fn init(cx: &mut App) {
    let ws = Some(WORKSPACE);
    let panel = Some(PROJECT_PANEL);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        // macOS's Settings key; in any window, the empty one too.
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-o", OpenFolder, ws),
        KeyBinding::new("cmd-shift-n", NewWindow, ws),
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
        KeyBinding::new("shift-up", ExtendPrev, panel),
        KeyBinding::new("shift-down", ExtendNext, panel),
        KeyBinding::new("right", Expand, panel),
        KeyBinding::new("left", Collapse, panel),
        KeyBinding::new("home", SelectFirst, panel),
        KeyBinding::new("end", SelectLast, panel),
        KeyBinding::new("pageup", PageUp, panel),
        KeyBinding::new("pagedown", PageDown, panel),
        KeyBinding::new("enter", Open, panel),
        KeyBinding::new("space", PassThrough, panel),
        KeyBinding::new("cmd-f", SpeedSearch, panel),
        KeyBinding::new("escape", FocusEditor, panel),
        KeyBinding::new("cmd-shift-o", ToggleFinder, ws),
        KeyBinding::new("cmd-shift-f", ToggleSearch, ws),
        // gpui-kit binds cmd-shift-f to replace inside its editor ("Input"), which would win there; the
        // folder search takes it, and replace moves to cmd-r as in IntelliJ.
        KeyBinding::new("cmd-shift-f", ToggleSearch, Some("Input")),
        KeyBinding::new("cmd-r", gpui_kit::component::input::Replace, Some("Input")),
        KeyBinding::new("cmd-alt-r", RevealInFinder, ws),
        KeyBinding::new("cmd-n", NewFile, panel),
        KeyBinding::new("shift-f6", Rename, panel),
        KeyBinding::new("cmd-backspace", Trash, panel),
        KeyBinding::new("cmd-x", CutItems, panel),
        KeyBinding::new("cmd-c", CopyItems, panel),
        KeyBinding::new("cmd-v", PasteItems, panel),
        KeyBinding::new("f5", CopyTo, panel),
        KeyBinding::new("f6", MoveTo, panel),
        // IntelliJ's Expand All and Collapse All. Cmd+Shift+= also reads as Cmd++, which zooms in
        // elsewhere; in the tree it expands, and Cmd+= still zooms.
        KeyBinding::new("cmd-shift-=", ExpandAll, panel),
        KeyBinding::new("cmd-+", ExpandAll, panel),
        KeyBinding::new("cmd-shift--", CollapseAll, panel),
        KeyBinding::new("cmd-_", CollapseAll, panel),
        // A name being typed has its own Cmd+Z: its input is deeper and wins.
        KeyBinding::new("cmd-z", UndoChange, panel),
        // The recent folders on the empty window.
        KeyBinding::new("up", SelectPrev, Some("Workspace && empty")),
        KeyBinding::new("down", SelectNext, Some("Workspace && empty")),
        KeyBinding::new("enter", Open, Some("Workspace && empty")),
        // Markdown editing keys (Phase 2) go below this line, in their own block.
        // gpui-kit binds enter and tab on its editor element ("Input"), deeper than any container.
        // These match that same element, so they tie on depth and, added later, run first. When a
        // line is not a list item the handler propagates and GPUI moves on to gpui-kit's binding.
        KeyBinding::new("enter", Newline, Some(MARKDOWN_EDITOR)),
        KeyBinding::new("tab", IndentItem, Some(MARKDOWN_EDITOR)),
        KeyBinding::new("shift-tab", OutdentItem, Some(MARKDOWN_EDITOR)),
        KeyBinding::new("cmd-b", ToggleBold, Some(MARKDOWN_EDITOR)),
        KeyBinding::new("cmd-i", ToggleItalic, Some(MARKDOWN_EDITOR)),
        // The terminal (IntelliJ's Alt+F12). Keys without a binding go to the shell.
        KeyBinding::new("alt-f12", ToggleTerminal, ws),
        KeyBinding::new("cmd-c", CopySelection, Some(TERMINAL)),
        KeyBinding::new("cmd-v", Paste, Some(TERMINAL)),
        // gpui-kit's root moves focus with tab and shift-tab; in the terminal they are the shell's.
        KeyBinding::new("tab", SendTab, Some(TERMINAL)),
        KeyBinding::new("shift-tab", SendBackTab, Some(TERMINAL)),
        // More shells, as IntelliJ's terminal tabs: Cmd+T anywhere, the rest inside the terminal,
        // where they win over the editor's own Cmd+W and tab keys.
        KeyBinding::new("cmd-t", NewTerminal, ws),
        KeyBinding::new("cmd-w", CloseTerminal, Some(TERMINAL)),
        KeyBinding::new("cmd-shift-[", PrevTerminal, Some(TERMINAL)),
        KeyBinding::new("cmd-shift-]", NextTerminal, Some(TERMINAL)),
    ]);
}
