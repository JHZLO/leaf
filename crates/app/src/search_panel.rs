//! Find in the folder (Cmd+Shift+F): lines that match, grouped by file; Enter opens the file at the line.

use std::path::PathBuf;

use gpui_kit::*;

use crate::theme::{self, c, size, z};

#[expect(dead_code, reason = "emitted once the search UI is built (Phase 3)")]
pub enum SearchEvent {
    /// Open this file (relative to the open folder) with the cursor on this zero-based line.
    Open { path: PathBuf, line: u32 },
    /// Escape: close and give focus back to where it was.
    Dismiss,
    /// Focus went elsewhere: close and leave it there.
    FocusLost,
}

pub struct SearchPanel {
    root: PathBuf,
    focus_handle: FocusHandle,
}

impl EventEmitter<SearchEvent> for SearchPanel {}

impl Focusable for SearchPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SearchPanel {
    pub fn new(root: PathBuf, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            root,
            focus_handle: cx.focus_handle(),
        }
    }
}

/// Places the panel over the window, below the title bar, like the finder.
pub fn overlay(panel: Entity<SearchPanel>, cx: &App) -> impl IntoElement {
    div()
        .absolute()
        .top(z(size::TITLE_BAR + 8., cx))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(panel)
}

impl Render for SearchPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        div()
            .track_focus(&self.focus_handle)
            .w(z(720., cx))
            .p(z(12., cx))
            .bg(c(p.paper))
            .border_1()
            .border_color(c(p.line))
            .rounded(z(6., cx))
            .text_color(c(p.sub))
            .child(format!("Find in {}", self.root.display()))
    }
}
