//! Open files as tabs, each over one gpui-kit editor: the leaf-through tab, the paper sheet, saving.

use std::path::{Path, PathBuf};

use gpui_kit::component::{
    ActiveTheme as _,
    input::{Editor, EditorState, InputEvent, RopeExt as _},
};
use gpui_kit::{prelude::FluentBuilder as _, *};
use leaf_project::fs::{self, FileStamp, LineEnding, MAX_HIGHLIGHT_BYTES};

use crate::{
    preview::Preview,
    theme::{self, c, size, z},
};

pub struct Tab {
    /// Relative to the open folder.
    pub path: PathBuf,
    pub editor: Entity<EditorState>,
    /// Opened with Space from the tree; replaced by the next leaf-through, kept once edited or opened with Enter.
    pub leaf_through: bool,
    pub dirty: bool,
    pub read_only: bool,
    pub line_ending: LineEnding,
    pub bom: bool,
    pub stamp: FileStamp,
    pub language: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl Tab {
    pub fn is_markdown(&self) -> bool {
        self.language == "markdown"
    }
}

/// Something the workspace shows (title, stem, status) may have changed.
pub struct TabsChanged;

pub struct EditorTabs {
    root: PathBuf,
    tabs: Vec<Tab>,
    active: Option<usize>,
    preview: Option<Entity<Preview>>,
    focus_handle: FocusHandle,
    /// The last file that could not be opened, and why; shown in the status bar.
    pub error: Option<String>,
}

impl EventEmitter<TabsChanged> for EditorTabs {}

impl Focusable for EditorTabs {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.active_tab() {
            Some(tab) => tab.editor.focus_handle(cx),
            None => self.focus_handle.clone(),
        }
    }
}

fn language_for(path: &Path) -> SharedString {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "md" | "markdown" | "mdx" => "markdown".into(),
        "yml" => "yaml".into(),
        "ts" | "mts" | "cts" => "typescript".into(),
        "js" | "mjs" | "cjs" => "javascript".into(),
        "" => "text".into(),
        other => other.to_string().into(),
    }
}

impl EditorTabs {
    pub fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        Self {
            root,
            tabs: Vec::new(),
            active: None,
            preview: None,
            focus_handle: cx.focus_handle(),
            error: None,
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|i| self.tabs.get(i))
    }

    #[expect(
        dead_code,
        reason = "asked before closing the window or switching folders (Phase 1)"
    )]
    pub fn has_unsaved(&self) -> bool {
        self.tabs.iter().any(|t| t.dirty)
    }

    /// The cursor or selection lines of the active tab, counted from 1.
    pub fn active_lines(&self, cx: &App) -> Option<(u32, u32)> {
        let tab = self.active_tab()?;
        let state = tab.editor.read(cx);
        let range = state.selected_range();
        let start = state.text().offset_to_position(range.start).line + 1;
        let mut end = state.text().offset_to_position(range.end).line + 1;
        // A selection that ends at the start of a line does not include that line.
        if end > start && state.text().offset_to_position(range.end).character == 0 {
            end -= 1;
        }
        Some((start, end))
    }

    /// Opens `path` (relative to the open folder), or switches to it when it is already open.
    pub fn open(
        &mut self,
        path: &Path,
        leaf_through: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.tabs.iter().position(|t| t.path == path) {
            if !leaf_through {
                self.tabs[index].leaf_through = false;
            }
            self.activate(index, window, cx);
            return;
        }
        let file = match fs::read_text(&self.root.join(path)) {
            Ok(file) => file,
            Err(err) => {
                self.error = Some(format!("{}: {err}", path.display()));
                cx.emit(TabsChanged);
                cx.notify();
                return;
            }
        };
        self.error = None;
        let mut language = language_for(path);
        if file.stamp.len > MAX_HIGHLIGHT_BYTES {
            language = "text".into();
        }
        let markdown = language == "markdown";
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(language.clone())
                .line_number(true)
                .soft_wrap(markdown)
                .default_value(file.text)
        });
        let subscriptions = vec![
            cx.subscribe(&editor, |this, editor, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    if let Some(tab) = this.tabs.iter_mut().find(|t| t.editor == editor) {
                        tab.dirty = true;
                        tab.leaf_through = false;
                    }
                    this.refresh_preview(cx);
                    cx.emit(TabsChanged);
                    cx.notify();
                }
            }),
            cx.observe(&editor, |_, _, cx| cx.emit(TabsChanged)),
        ];
        let tab = Tab {
            path: path.to_path_buf(),
            editor,
            leaf_through,
            dirty: false,
            read_only: file.mixed_line_endings,
            line_ending: file.line_ending,
            bom: file.bom,
            stamp: file.stamp,
            language,
            _subscriptions: subscriptions,
        };
        // A new leaf-through replaces the previous one in place.
        let index = match self.tabs.iter().position(|t| t.leaf_through) {
            Some(i) if leaf_through => {
                self.tabs[i] = tab;
                i
            }
            _ => {
                self.tabs.push(tab);
                self.tabs.len() - 1
            }
        };
        self.activate(index, window, cx);
    }

    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.active = Some(index);
        let _ = window;
        self.refresh_preview(cx);
        cx.emit(TabsChanged);
        cx.notify();
    }

    pub fn focus_active(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle(cx).focus(window, cx);
    }

    pub fn activate_offset(&mut self, offset: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.active else { return };
        let len = self.tabs.len() as isize;
        let next = (active as isize + offset).rem_euclid(len) as usize;
        self.activate(next, window, cx);
        self.focus_active(window, cx);
    }

    pub fn save_active(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(index) = self.active else {
            return Ok(());
        };
        let tab = &self.tabs[index];
        if tab.read_only {
            return Err("mixed line endings: read-only".into());
        }
        let text = tab.editor.read(cx).value().to_string();
        let path = fs::write_target(&self.root.join(&tab.path));
        let stamp =
            fs::write_text(&path, &text, tab.line_ending, tab.bom).map_err(|e| e.to_string())?;
        let tab = &mut self.tabs[index];
        tab.stamp = stamp;
        tab.dirty = false;
        tab.leaf_through = false;
        cx.emit(TabsChanged);
        cx.notify();
        Ok(())
    }

    pub fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.active else { return };
        self.tabs.remove(index);
        self.active = if self.tabs.is_empty() {
            None
        } else {
            Some(index.min(self.tabs.len() - 1))
        };
        self.refresh_preview(cx);
        self.focus_active(window, cx);
        cx.emit(TabsChanged);
        cx.notify();
    }

    /// Files changed on disk outside leaf.
    #[expect(dead_code, reason = "called by the folder watcher (Phase 1)")]
    pub fn files_changed(
        &mut self,
        files: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = (files, window, cx);
    }

    pub fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        if self.preview.take().is_none() {
            let source = self.preview_source(cx).unwrap_or_default();
            self.preview = Some(cx.new(|cx| Preview::new(&source, cx)));
        }
        cx.notify();
    }

    fn preview_source(&self, cx: &App) -> Option<String> {
        let tab = self.active_tab().filter(|t| t.is_markdown())?;
        Some(tab.editor.read(cx).value().to_string())
    }

    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.preview.clone() else {
            return;
        };
        let source = self.preview_source(cx).unwrap_or_default();
        preview.update(cx, |preview, cx| preview.schedule(source, cx));
    }

    fn render_tab_bar(&self, cx: &App) -> impl IntoElement {
        let p = theme::colors(cx);
        div()
            .h(z(size::TAB_BAR, cx))
            .flex_none()
            .flex()
            .bg(c(p.side))
            .border_b_1()
            .border_color(c(p.line))
            .text_size(z(12.5, cx))
            .children(self.tabs.iter().enumerate().map(|(i, tab)| {
                let active = self.active == Some(i);
                let name = tab
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .border_r_1()
                    .border_color(c(p.line))
                    .text_color(c(if active { p.ink } else { p.sub }))
                    .when(active, |el| {
                        el.bg(c(p.paper)).border_b_2().border_color(c(p.accent))
                    })
                    .when(tab.leaf_through, |el| el.italic())
                    .child(name)
                    .when(tab.dirty, |el| {
                        el.child(div().size(px(6.)).rounded_full().bg(c(p.sub)))
                    })
            }))
    }
}

impl Render for EditorTabs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let body = match self.active_tab() {
            None => div().flex_1().bg(c(p.paper)).into_any_element(),
            Some(tab) => {
                let editor = Editor::new(&tab.editor)
                    .bordered(false)
                    .h_full()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(z(size::EDITOR_TEXT, cx));
                if tab.is_markdown() {
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .justify_center()
                        .px_5()
                        .bg(c(p.desk))
                        .child(
                            div()
                                .w_full()
                                .max_w(z(size::SHEET, cx))
                                .h_full()
                                .bg(c(p.paper))
                                .border_x_1()
                                .border_color(c(p.line))
                                .child(editor),
                        )
                        .into_any_element()
                } else {
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .bg(c(p.paper))
                        .child(editor)
                        .into_any_element()
                }
            }
        };
        div()
            .track_focus(&self.focus_handle)
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(self.render_tab_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(body)
                    .children(self.preview.clone()),
            )
    }
}
