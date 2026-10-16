//! The macOS menu bar. The menus show each action's key binding from keymap.rs.

use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::*;

use crate::keymap::{
    CloseTab, CopyPath, CopyRelativePath, FocusTree, NextTab, OpenFolder, PrevTab, Quit, Save,
    TogglePreview, ZoomIn, ZoomOut, ZoomReset,
};

fn menu(name: &str, items: Vec<MenuItem>) -> Menu {
    Menu {
        name: name.to_string().into(),
        items,
        disabled: false,
    }
}

pub fn init(cx: &mut App) {
    cx.set_menus(vec![
        menu("leaf", vec![MenuItem::action("Quit leaf", Quit)]),
        menu(
            "File",
            vec![
                MenuItem::action("Open Folder…", OpenFolder),
                MenuItem::separator(),
                MenuItem::action("Save", Save),
                MenuItem::action("Close Tab", CloseTab),
            ],
        ),
        menu(
            "Edit",
            vec![
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
                MenuItem::separator(),
                MenuItem::action("Copy Path", CopyPath),
                MenuItem::action("Copy Relative Path", CopyRelativePath),
            ],
        ),
        menu(
            "View",
            vec![
                MenuItem::action("Focus Tree", FocusTree),
                MenuItem::action("Toggle Preview", TogglePreview),
                MenuItem::separator(),
                MenuItem::action("Previous Tab", PrevTab),
                MenuItem::action("Next Tab", NextTab),
                MenuItem::separator(),
                MenuItem::action("Zoom In", ZoomIn),
                MenuItem::action("Zoom Out", ZoomOut),
                MenuItem::action("Actual Size", ZoomReset),
            ],
        ),
    ]);
}
