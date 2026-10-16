//! Open files as tabs, each over one gpui-kit editor: the leaf-through tab, the paper sheet, saving.

use std::path::{Path, PathBuf};

use gpui_kit::component::{
    ActiveTheme as _,
    input::{Editor, EditorState, InputEvent, RopeExt as _},
};
use gpui_kit::{prelude::FluentBuilder as _, *};
use leaf_project::fs::{self, FileStamp, LineEnding, MAX_HIGHLIGHT_BYTES, ReadError, TextFile};

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

/// A file being read on the background executor for [`EditorTabs::open`].
struct PendingOpen {
    id: u64,
    path: PathBuf,
    leaf_through: bool,
}

pub struct EditorTabs {
    root: PathBuf,
    tabs: Vec<Tab>,
    active: Option<usize>,
    pending: Vec<PendingOpen>,
    next_open: u64,
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
            pending: Vec::new(),
            next_open: 0,
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
    /// The file is read off the main thread; the tab appears when it has been read.
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
        if let Some(pending) = self.pending.iter_mut().find(|p| p.path == path) {
            // Already being read; Enter on a file still loading from Space makes it a normal tab.
            pending.leaf_through &= leaf_through;
            return;
        }
        if leaf_through {
            // Walking the tree with Space, only the newest file is worth showing.
            self.pending.retain(|p| !p.leaf_through);
        }
        let id = self.next_open;
        self.next_open += 1;
        self.pending.push(PendingOpen {
            id,
            path: path.to_path_buf(),
            leaf_through,
        });
        let abs = self.root.join(path);
        let read = cx
            .background_executor()
            .spawn(async move { fs::read_text(&abs) });
        cx.spawn_in(window, async move |this, cx| {
            let file = read.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_open(id, file, window, cx)
            });
        })
        .detach();
    }

    fn finish_open(
        &mut self,
        id: u64,
        file: Result<TextFile, ReadError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A newer leaf-through may have replaced this request while the file was read.
        let Some(ix) = self.pending.iter().position(|p| p.id == id) else {
            return;
        };
        let PendingOpen {
            path, leaf_through, ..
        } = self.pending.remove(ix);
        let file = match file {
            Ok(file) => file,
            Err(err) => {
                self.error = Some(format!("{}: {err}", path.display()));
                cx.emit(TabsChanged);
                cx.notify();
                return;
            }
        };
        self.error = None;
        let tab = self.new_tab(path, leaf_through, file, window, cx);
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

    fn new_tab(
        &mut self,
        path: PathBuf,
        leaf_through: bool,
        file: TextFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Tab {
        let mut language = language_for(&path);
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
                    this.edited(&editor, cx);
                }
            }),
            cx.observe(&editor, |_, _, cx| cx.emit(TabsChanged)),
        ];
        Tab {
            path,
            editor,
            leaf_through,
            dirty: false,
            read_only: file.mixed_line_endings,
            line_ending: file.line_ending,
            bom: file.bom,
            stamp: file.stamp,
            language,
            _subscriptions: subscriptions,
        }
    }

    fn edited(&mut self, editor: &Entity<EditorState>, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| &t.editor == editor) {
            tab.dirty = true;
            tab.leaf_through = false;
        }
        self.refresh_preview(cx);
        cx.emit(TabsChanged);
        cx.notify();
    }

    fn activate(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        // Focus follows the switch only when it was already in the editor, so Space in the tree keeps it there.
        let had_focus = self.focus_handle.contains_focused(window, cx);
        self.active = Some(index);
        if had_focus {
            self.focus_active(window, cx);
        }
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

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let active = self.active == Some(i);
                let name = tab
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                div()
                    .id(("tab", i))
                    .relative()
                    .flex_none()
                    .h_full()
                    .flex()
                    .items_center()
                    .gap(z(7., cx))
                    .px(z(16., cx))
                    .border_r_1()
                    .border_color(c(p.line))
                    .text_color(c(if active { p.ink } else { p.sub }))
                    .map(|el| {
                        if active {
                            el.bg(c(p.paper))
                        } else {
                            el.hover(|style| style.bg(c(p.hover)))
                        }
                    })
                    .when(tab.leaf_through, |el| el.italic())
                    .child(name)
                    .when(tab.dirty, |el| {
                        el.child(
                            div()
                                .flex_none()
                                .size(z(6., cx))
                                .rounded_full()
                                .bg(c(p.sub)),
                        )
                    })
                    // Drawn over the tab rather than as a border, so the active tab is as tall as the rest.
                    .when(active, |el| {
                        el.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(z(2., cx))
                                .bg(c(p.accent)),
                        )
                    })
                    .when(tab.leaf_through, |el| {
                        el.child(folded_corner(z(9., cx), c(p.fold)))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        if i < this.tabs.len() {
                            this.activate(i, window, cx);
                            this.focus_active(window, cx);
                        }
                    }))
            })
            .collect();
        div()
            .h(z(size::TAB_BAR, cx))
            .flex_none()
            .flex()
            .overflow_x_hidden()
            .bg(c(p.side))
            .border_b_1()
            .border_color(c(p.line))
            .text_size(z(12.5, cx))
            .children(tabs)
    }
}

/// The leaf-through mark: the tab's top right corner folded over, a right triangle of `size`.
fn folded_corner(size: Pixels, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let (right, top) = (bounds.right(), bounds.top());
            let mut corner = gpui_kit::Path::new(point(right - size, top));
            corner.line_to(point(right, top));
            corner.line_to(point(right, top + size));
            window.paint_path(corner, color);
        },
    )
    .absolute()
    .top_0()
    .right_0()
    .size(size)
}

impl Render for EditorTabs {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let body = match self.active_tab() {
            None => div().flex_1().bg(c(p.paper)).into_any_element(),
            Some(tab) => {
                let editor = Editor::new(&tab.editor)
                    .bordered(false)
                    .readonly(tab.read_only)
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
