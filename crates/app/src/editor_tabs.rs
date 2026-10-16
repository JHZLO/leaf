//! Open files as tabs, each over one gpui-kit editor: the leaf-through tab, the paper sheet, saving.

use std::{
    io,
    ops::Range,
    path::{Path, PathBuf},
};

use gpui_kit::component::{
    ActiveTheme as _, Sizable as _, WindowExt as _,
    button::Button,
    dialog::{DialogEntrance, DialogFooter},
    input::{Editor, EditorState, InputEvent, Position, Rope, RopeExt as _, TabSize},
};
use gpui_kit::{prelude::FluentBuilder as _, *};
use leaf_project::{
    fs::{self, FileStamp, LineEnding, MAX_HIGHLIGHT_BYTES, ReadError, TextFile},
    markdown::{self, Edit, Emphasis},
    session::TabState,
};

use crate::{
    keymap::{IndentItem, MARKDOWN, Newline, OutdentItem, ToggleBold, ToggleItalic},
    preview::Preview,
    theme::{self, c, size, z},
};

/// How far up and down from a list item Tab looks for its list's indent unit; nearby lines
/// show it, and the bound keeps the key cheap in a long block.
const LIST_LOOKAROUND: usize = 100;

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
    /// The file is gone from disk; saving writes it again.
    deleted: bool,
    /// The file changed on disk under unsaved edits: its stamp there, until the user picks a side.
    conflict: Option<FileStamp>,
    /// Counts edits, so a save that finishes after more typing leaves the tab dirty.
    edits: u64,
    saving: bool,
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
    /// Where to put the cursor once open (zero-based line and column), for a restored session.
    cursor: Option<(u32, u32)>,
}

pub struct EditorTabs {
    root: PathBuf,
    tabs: Vec<Tab>,
    active: Option<usize>,
    pending: Vec<PendingOpen>,
    next_open: u64,
    /// While a session's tabs load, only this one takes the active place, so the order reads finish in does not matter.
    restoring_active: Option<PathBuf>,
    /// The active tab's cursor lines as last reported, so caret blinks do not count as changes.
    last_lines: Option<(u32, u32)>,
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

fn file_name(path: &Path) -> SharedString {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
        .into()
}

/// The active tab after the tab at `closed` is removed, leaving `len` tabs.
fn active_after_close(active: Option<usize>, closed: usize, len: usize) -> Option<usize> {
    let active = active?;
    if len == 0 {
        None
    } else if active > closed {
        Some(active - 1)
    } else {
        Some(active.min(len - 1))
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
            restoring_active: None,
            last_lines: None,
            preview: None,
            focus_handle: cx.focus_handle(),
            error: None,
        }
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|i| self.tabs.get(i))
    }

    /// How many tabs have edits that are not on disk.
    pub fn tabs_unsaved(&self) -> usize {
        self.tabs.iter().filter(|t| t.dirty).count()
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
            cursor: None,
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
            path,
            leaf_through,
            cursor,
            ..
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
        if let Some((line, column)) = cursor {
            tab.editor.update(cx, |state, cx| {
                state.set_cursor_position(Position::new(line, column), window, cx)
            });
        }
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
        match &self.restoring_active {
            Some(active) if *active != self.tabs[index].path && self.active.is_some() => {
                cx.emit(TabsChanged);
                cx.notify();
            }
            _ => {
                if self.restoring_active.as_ref() == Some(&self.tabs[index].path) {
                    self.restoring_active = None;
                }
                self.activate(index, window, cx);
            }
        }
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
                .soft_wrap(crate::settings(cx).wraps(markdown))
                .tab_size(TabSize {
                    tab_size: crate::settings(cx).tab_size,
                    hard_tabs: false,
                })
                .default_value(file.text)
        });
        let subscriptions = vec![
            cx.subscribe(&editor, |this, editor, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.edited(&editor, cx);
                }
            }),
            // The editor notifies on every caret blink; pass on only a move to another line.
            cx.observe(&editor, |this, editor, cx| {
                if this.active_tab().is_some_and(|t| t.editor == editor) {
                    let lines = this.active_lines(cx);
                    if lines != this.last_lines {
                        this.last_lines = lines;
                        cx.emit(TabsChanged);
                    }
                }
            }),
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
            deleted: false,
            conflict: None,
            edits: 0,
            saving: false,
            _subscriptions: subscriptions,
        }
    }

    fn edited(&mut self, editor: &Entity<EditorState>, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| &t.editor == editor) {
            tab.dirty = true;
            tab.leaf_through = false;
            tab.edits += 1;
        }
        if let Some(preview) = self.preview.clone()
            && self.preview_source().as_ref() == Some(editor)
        {
            preview.update(cx, |preview, cx| preview.schedule(cx));
        }
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

    /// Saves the active tab off the main thread. Failures go to [`Self::error`].
    pub fn save_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab() {
            let id = tab.editor.entity_id();
            self.save(id, false, false, window, cx);
        }
    }

    /// Writes the tab to disk. Unless `overwrite`, asks first when the file changed on disk since it
    /// was read or written; `then_close` closes the tab once the file is written.
    fn save(
        &mut self,
        id: EntityId,
        overwrite: bool,
        then_close: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let tab = &mut self.tabs[index];
        if tab.read_only {
            self.error = Some(format!(
                "{}: mixed line endings, read-only",
                tab.path.display()
            ));
            cx.emit(TabsChanged);
            cx.notify();
            return;
        }
        // One write per tab at a time, so an older text never lands after a newer one.
        if tab.saving {
            return;
        }
        tab.saving = true;
        let edits = tab.edits;
        // The rope is shared, not copied; it becomes a string on the background executor.
        let text = tab.editor.read(cx).text().clone();
        let path = self.root.join(&tab.path);
        // A deleted file is written again without asking.
        let expected = (!overwrite && !tab.deleted).then_some(tab.stamp);
        let (line_ending, bom) = (tab.line_ending, tab.bom);
        let task = cx
            .background_executor()
            .spawn(async move { write_file(&path, &text.to_string(), expected, line_ending, bom) });
        cx.spawn_in(window, async move |this, cx| {
            let saved = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_save(id, edits, then_close, saved, window, cx)
            });
        })
        .detach();
    }

    fn finish_save(
        &mut self,
        id: EntityId,
        edits: u64,
        then_close: bool,
        saved: Saved,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let tab = &mut self.tabs[index];
        tab.saving = false;
        match saved {
            Saved::Written(stamp) => {
                tab.stamp = stamp;
                tab.deleted = false;
                tab.conflict = None;
                tab.leaf_through = false;
                // Typing while the file was written leaves edits the file does not have yet.
                if tab.edits == edits {
                    tab.dirty = false;
                }
                self.error = None;
                if then_close {
                    self.remove_tab(index, window, cx);
                }
            }
            Saved::ChangedOnDisk => self.confirm_overwrite(id, then_close, window, cx),
            Saved::Failed(err) => {
                self.error = Some(format!("{}: {err}", tab.path.display()));
            }
        }
        cx.emit(TabsChanged);
        cx.notify();
    }

    fn confirm_overwrite(
        &mut self,
        id: EntityId,
        then_close: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let name = file_name(&self.tabs[index].path);
        let this = cx.entity().downgrade();
        // Each choice closes the dialog itself first, so focus is back in the editor before the tab changes.
        let overwrite = move |window: &mut Window, cx: &mut App| {
            window.close_dialog(cx);
            let _ = this.update(cx, |tabs, cx| tabs.save(id, true, then_close, window, cx));
        };
        window.open_alert_dialog(cx, move |alert, _, cx| {
            let ok = overwrite.clone();
            let click = overwrite.clone();
            alert
                .title(format!("{name} changed on disk"))
                .description("Saving replaces the version on disk with yours.")
                .entrance(DialogEntrance::None)
                .on_ok(move |_, window, cx| {
                    ok(window, cx);
                    false
                })
                .footer(
                    DialogFooter::new()
                        .child(
                            leaf_button("cancel", "Cancel", cx)
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            leaf_button("overwrite", "Overwrite", cx)
                                .on_click(move |_, window, cx| click(window, cx)),
                        ),
                )
        });
    }

    /// The tree renamed `from` (a file or a folder) to `to`; tabs inside follow the new path.
    pub fn renamed(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        let moved = |path: &mut PathBuf| {
            if let Ok(rest) = path.strip_prefix(from) {
                *path = to.join(rest);
            }
        };
        self.tabs.iter_mut().for_each(|tab| moved(&mut tab.path));
        self.pending
            .iter_mut()
            .for_each(|open| moved(&mut open.path));
        cx.emit(TabsChanged);
        cx.notify();
    }

    /// The tree moved `path` to the Trash: clean tabs inside close; edited ones stay, marked deleted,
    /// so nothing typed is lost.
    pub fn trashed(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        for index in (0..self.tabs.len()).rev() {
            if !self.tabs[index].path.starts_with(path) {
                continue;
            }
            if self.tabs[index].dirty {
                self.tabs[index].deleted = true;
            } else {
                self.remove_tab(index, window, cx);
            }
        }
        cx.emit(TabsChanged);
        cx.notify();
    }

    /// Closes the active tab, asking first when it has unsaved edits.
    pub fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab() else { return };
        let id = tab.editor.entity_id();
        if tab.dirty {
            self.confirm_close(id, window, cx);
        } else if let Some(index) = self.index_of(id) {
            self.remove_tab(index, window, cx);
        }
    }

    fn confirm_close(&mut self, id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let name = file_name(&self.tabs[index].path);
        let this = cx.entity().downgrade();
        let save = {
            let this = this.clone();
            move |window: &mut Window, cx: &mut App| {
                window.close_dialog(cx);
                let _ = this.update(cx, |tabs, cx| tabs.save(id, false, true, window, cx));
            }
        };
        let discard = move |window: &mut Window, cx: &mut App| {
            window.close_dialog(cx);
            let _ = this.update(cx, |tabs, cx| {
                if let Some(index) = tabs.index_of(id) {
                    tabs.remove_tab(index, window, cx);
                }
            });
        };
        window.open_alert_dialog(cx, move |alert, _, cx| {
            let ok = save.clone();
            let click = save.clone();
            let discard = discard.clone();
            alert
                .title(format!("Save changes to {name}?"))
                .description("Your changes are lost if you don't save them.")
                .entrance(DialogEntrance::None)
                .on_ok(move |_, window, cx| {
                    ok(window, cx);
                    false
                })
                .footer(
                    DialogFooter::new()
                        .child(
                            leaf_button("dont-save", "Don't save", cx)
                                .on_click(move |_, window, cx| discard(window, cx)),
                        )
                        .child(
                            leaf_button("cancel", "Cancel", cx)
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            leaf_button("save", "Save", cx)
                                .on_click(move |_, window, cx| click(window, cx)),
                        ),
                )
        });
    }

    fn remove_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let had_focus = self.focus_handle.contains_focused(window, cx);
        self.tabs.remove(index);
        self.active = active_after_close(self.active, index, self.tabs.len());
        if had_focus {
            self.focus_active(window, cx);
        }
        self.refresh_preview(cx);
        cx.emit(TabsChanged);
        cx.notify();
    }

    /// Files changed on disk outside leaf (relative to the open folder, as the watcher reports them).
    /// A clean tab reloads, a tab with edits shows a banner, a deleted file marks its tab.
    pub fn files_changed(
        &mut self,
        files: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A folder that was renamed or removed counts for every file under it.
        let checks: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| files.iter().any(|f| t.path.starts_with(f)))
            .map(|t| {
                (
                    t.editor.entity_id(),
                    self.root.join(&t.path),
                    t.stamp,
                    t.dirty,
                )
            })
            .collect();
        if checks.is_empty() {
            return;
        }
        let task = cx.background_executor().spawn(async move {
            checks
                .into_iter()
                .map(|(id, path, stamp, dirty)| (id, check_disk(&path, stamp, dirty)))
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let states = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                for (id, state) in states {
                    this.apply_disk_state(id, state, window, cx);
                }
                cx.emit(TabsChanged);
                cx.notify();
            });
        })
        .detach();
    }

    fn index_of(&self, id: EntityId) -> Option<usize> {
        self.tabs.iter().position(|t| t.editor.entity_id() == id)
    }

    fn apply_disk_state(
        &mut self,
        id: EntityId,
        state: DiskState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let tab = &mut self.tabs[index];
        // The tab may have saved or reloaded while the disk was checked.
        let seen = match &state {
            DiskState::Changed(stamp) => Some(*stamp),
            DiskState::Reread(Ok(file)) => Some(file.stamp),
            _ => None,
        };
        if seen == Some(tab.stamp) {
            tab.deleted = false;
            return;
        }
        match state {
            DiskState::Same => tab.deleted = false,
            DiskState::Gone => {
                tab.deleted = true;
                tab.conflict = None;
            }
            DiskState::Changed(stamp) if tab.dirty => {
                tab.deleted = false;
                tab.conflict = Some(stamp);
            }
            DiskState::Changed(_) => self.reload_from_disk(id, window, cx),
            DiskState::Reread(Ok(file)) if tab.dirty => {
                tab.deleted = false;
                tab.conflict = Some(file.stamp);
            }
            DiskState::Reread(Ok(file)) => self.reload(index, file, window, cx),
            DiskState::Reread(Err(err)) => {
                self.error = Some(format!("{}: {err}", tab.path.display()));
            }
        }
    }

    /// Replaces the tab's text with `file`, keeping the cursor where it was (on the last line if the file got shorter).
    fn reload(
        &mut self,
        index: usize,
        file: TextFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab = &mut self.tabs[index];
        tab.editor.update(cx, |state, cx| {
            let mut cursor = state.cursor_position();
            state.set_value(file.text, window, cx);
            let last_line = state.text().lines_len().saturating_sub(1);
            cursor.line = cursor.line.min(last_line as u32);
            let offset = state.text().position_to_offset(&cursor);
            state.set_selected_range(offset..offset, cx);
        });
        tab.stamp = file.stamp;
        tab.line_ending = file.line_ending;
        tab.bom = file.bom;
        tab.read_only = file.mixed_line_endings;
        tab.dirty = false;
        tab.deleted = false;
        tab.conflict = None;
        if self.active == Some(index) {
            self.refresh_preview(cx);
        }
    }

    fn reload_from_disk(&mut self, id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let path = self.root.join(&self.tabs[index].path);
        let read = cx
            .background_executor()
            .spawn(async move { fs::read_text(&path) });
        cx.spawn_in(window, async move |this, cx| {
            let file = read.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(index) = this.index_of(id) else {
                    return;
                };
                match file {
                    Ok(file) => this.reload(index, file, window, cx),
                    Err(ReadError::Io(err)) if err.kind() == io::ErrorKind::NotFound => {
                        this.tabs[index].deleted = true;
                        this.tabs[index].conflict = None;
                    }
                    Err(err) => {
                        this.error = Some(format!("{}: {err}", this.tabs[index].path.display()));
                    }
                }
                cx.emit(TabsChanged);
                cx.notify();
            });
        })
        .detach();
    }

    /// Keeps the edits over a file changed on disk; saving then writes them without asking again.
    fn keep_mine(&mut self, id: EntityId, cx: &mut Context<Self>) {
        let Some(index) = self.index_of(id) else {
            return;
        };
        let tab = &mut self.tabs[index];
        if let Some(stamp) = tab.conflict.take() {
            tab.stamp = stamp;
        }
        cx.notify();
    }

    fn render_banner(&self, id: EntityId, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = theme::colors(cx);
        div()
            .flex_none()
            .h(z(size::TAB_BAR, cx))
            .flex()
            .items_center()
            .justify_between()
            .px(z(16., cx))
            .bg(c(p.side))
            .border_b_1()
            .border_color(c(p.line))
            .text_size(z(12.5, cx))
            .text_color(c(p.ink))
            .child("Changed on disk")
            .child(
                div()
                    .flex()
                    .gap(z(8., cx))
                    .child(leaf_button("reload", "Reload", cx).on_click(cx.listener(
                        move |this, _: &ClickEvent, window, cx| {
                            this.reload_from_disk(id, window, cx)
                        },
                    )))
                    .child(leaf_button("keep-mine", "Keep mine", cx).on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| this.keep_mine(id, cx)),
                    )),
            )
    }

    /// Reopens a saved session's tabs, each at its cursor, with `active` as the active tab.
    pub fn restore(
        &mut self,
        tabs: &[TabState],
        active: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restoring_active = active.and_then(|i| tabs.get(i)).map(|t| t.path.clone());
        for tab in tabs {
            self.open(&tab.path, false, window, cx);
            if let Some(pending) = self.pending.iter_mut().find(|p| p.path == tab.path) {
                pending.cursor = Some((tab.line, tab.column));
            }
        }
    }

    /// The tabs as a session keeps them, and which one is active.
    pub fn session_tabs(&self, cx: &App) -> (Vec<TabState>, Option<usize>) {
        let tabs = self
            .tabs
            .iter()
            .map(|tab| {
                let cursor = tab.editor.read(cx).cursor_position();
                TabState {
                    path: tab.path.clone(),
                    line: cursor.line,
                    column: cursor.character,
                }
            })
            .collect();
        (tabs, self.active)
    }

    pub fn preview_open(&self) -> bool {
        self.preview.is_some()
    }

    pub fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        if self.preview.take().is_none() {
            self.preview = Some(cx.new(Preview::new));
            self.refresh_preview(cx);
        }
        cx.notify();
    }

    /// The editor the preview shows: the active tab, when it is Markdown.
    fn preview_source(&self) -> Option<Entity<EditorState>> {
        self.active_tab()
            .filter(|t| t.is_markdown())
            .map(|t| t.editor.clone())
    }

    /// Shows the active tab in the preview at once (switching tabs is not typing).
    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.preview.clone() else {
            return;
        };
        let source = self.preview_source();
        preview.update(cx, |preview, cx| preview.show(source, cx));
    }

    /// The active tab's editor when it is Markdown, has the keyboard and takes edits. Anything
    /// else (the find bar, a read-only file, an IME composition) keeps gpui-kit's own keys.
    fn markdown_editor(&self, window: &mut Window, cx: &mut App) -> Option<Entity<EditorState>> {
        let tab = self
            .active_tab()
            .filter(|t| t.is_markdown() && !t.read_only)?;
        let editor = tab.editor.clone();
        if !editor.focus_handle(cx).is_focused(window) {
            return None;
        }
        let composing = editor.update(cx, |state, cx| {
            state.marked_text_range(window, cx).is_some()
        });
        (!composing).then_some(editor)
    }

    /// Runs a Markdown editing key. `edit` gets the text and the selection and says what to
    /// change, relative to an offset. With nothing to change the key propagates, and GPUI goes on
    /// to gpui-kit's binding for the same key (its newline, its tab).
    fn edit_markdown(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        edit: impl FnOnce(&Rope, Range<usize>) -> Option<(usize, Edit)>,
    ) {
        let applied = self.markdown_editor(window, cx).is_some_and(|editor| {
            editor.update(cx, |state, cx| {
                let Some((base, edit)) = edit(state.text(), state.selected_range()) else {
                    return false;
                };
                // A single replace, so one undo takes it back.
                state.set_selected_range(base + edit.range.start..base + edit.range.end, cx);
                state.replace(edit.text, window, cx);
                let selection = edit.selection;
                state.set_selected_range(base + selection.start..base + selection.end, cx);
                true
            })
        });
        if !applied {
            cx.propagate();
        }
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_markdown(window, cx, |text, selection| {
            if !selection.is_empty() {
                return None;
            }
            let (_, start, line) = line_at(text, selection.start);
            markdown::enter(&line, selection.start - start).map(|edit| (start, edit))
        });
    }

    fn indent_item(&mut self, _: &IndentItem, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_markdown(window, cx, |text, selection| {
            let (row, start, line) = single_line(text, &selection)?;
            let local = selection.start - start..selection.end - start;
            markdown::indent(&line, local, &list_unit(text, row)).map(|edit| (start, edit))
        });
    }

    fn outdent_item(&mut self, _: &OutdentItem, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_markdown(window, cx, |text, selection| {
            let (row, start, line) = single_line(text, &selection)?;
            let local = selection.start - start..selection.end - start;
            markdown::outdent(&line, local, &list_unit(text, row)).map(|edit| (start, edit))
        });
    }

    fn toggle_emphasis(&mut self, emphasis: Emphasis, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_markdown(window, cx, |text, selection| {
            let (_, start, _) = line_at(text, selection.start);
            let end = text.line_end_offset(text.offset_to_point(selection.end).row);
            let lines = text.slice(start..end).to_string();
            let local = selection.start - start..selection.end - start;
            Some((start, markdown::toggle_emphasis(&lines, local, emphasis)))
        });
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let active = self.active == Some(i);
                let name = file_name(&tab.path);
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
                    .text_color(c(if active && !tab.deleted { p.ink } else { p.sub }))
                    .map(|el| {
                        if active {
                            el.bg(c(p.paper))
                        } else {
                            el.hover(|style| style.bg(c(p.hover)))
                        }
                    })
                    .when(tab.leaf_through, |el| el.italic())
                    .child(div().when(tab.deleted, |el| el.line_through()).child(name))
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

/// The line holding `offset`: its row, where it starts, and its text.
fn line_at(text: &Rope, offset: usize) -> (usize, usize, String) {
    let row = text.offset_to_point(offset).row;
    (
        row,
        text.line_start_offset(row),
        text.slice_line(row).to_string(),
    )
}

/// [`line_at`] for a selection on one line; a selection over several lines is gpui-kit's to indent.
fn single_line(text: &Rope, selection: &Range<usize>) -> Option<(usize, usize, String)> {
    let line = line_at(text, selection.start);
    (text.offset_to_point(selection.end).row == line.0).then_some(line)
}

/// The indent unit of the list around `row`, read from its lines up to a blank line each way.
fn list_unit(text: &Rope, row: usize) -> String {
    let line = |r: usize| text.slice_line(r).to_string();
    let filled = |l: &String| !l.trim().is_empty();
    let mut lines: Vec<String> = (row.saturating_sub(LIST_LOOKAROUND)..row)
        .rev()
        .map(line)
        .take_while(filled)
        .collect();
    lines.reverse();
    let end = text.lines_len().min(row + LIST_LOOKAROUND);
    lines.extend((row..end).map(line).take_while(filled));
    markdown::indent_unit(lines.iter().map(String::as_str))
}

/// leaf's one button look (the design's Open folder button): `side` fill, `line` border, radius 6.
fn leaf_button(id: &'static str, label: &'static str, cx: &App) -> Button {
    let p = theme::colors(cx);
    Button::new(id)
        .label(label)
        .small()
        .rounded(z(6., cx))
        .bg(c(p.side))
        .text_color(c(p.ink))
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
        // In a column under the banner, so it takes the height left rather than all of it.
        let body = div().flex_1().min_h_0();
        let body = match self.active_tab() {
            None => body.bg(c(p.paper)),
            Some(tab) => {
                let editor = Editor::new(&tab.editor)
                    .bordered(false)
                    .readonly(tab.read_only)
                    .h_full()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(z(theme::editor_text(cx), cx));
                if tab.is_markdown() {
                    // The paper sheet: a reading-width page on the desk, keeping 20 px of desk on each
                    // side when the pane is narrower than the page.
                    body.key_context(MARKDOWN)
                        .on_action(cx.listener(Self::newline))
                        .on_action(cx.listener(Self::indent_item))
                        .on_action(cx.listener(Self::outdent_item))
                        .on_action(cx.listener(|this, _: &ToggleBold, window, cx| {
                            this.toggle_emphasis(Emphasis::Bold, window, cx)
                        }))
                        .on_action(cx.listener(|this, _: &ToggleItalic, window, cx| {
                            this.toggle_emphasis(Emphasis::Italic, window, cx)
                        }))
                        .flex()
                        .justify_center()
                        .px(z(20., cx))
                        .bg(c(p.desk))
                        .child(
                            div()
                                .w_full()
                                .max_w(z(size::SHEET, cx))
                                .h_full()
                                .py(z(20., cx))
                                .bg(c(p.paper))
                                .border_x_1()
                                .border_color(c(p.line))
                                .child(editor),
                        )
                } else {
                    body.bg(c(p.paper)).child(editor)
                }
            }
        };
        let banner = self
            .active_tab()
            .filter(|t| t.conflict.is_some())
            .map(|t| t.editor.entity_id())
            .map(|id| self.render_banner(id, cx));
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .children(banner)
                            .child(body),
                    )
                    .children(self.preview.clone()),
            )
    }
}

/// How a save ended.
#[derive(Debug)]
enum Saved {
    Written(FileStamp),
    /// The file is no longer what the tab read or last wrote; nothing was written.
    ChangedOnDisk,
    Failed(String),
}

/// Writes `text` to `path` (a symlink's target), unless the file's stamp is no longer `expected`.
/// Runs on the background executor.
fn write_file(
    path: &Path,
    text: &str,
    expected: Option<FileStamp>,
    line_ending: LineEnding,
    bom: bool,
) -> Saved {
    let target = fs::write_target(path);
    if let Some(expected) = expected
        && let Ok(stamp) = fs::stamp(&target)
        && stamp != expected
    {
        return Saved::ChangedOnDisk;
    }
    match fs::write_text(&target, text, line_ending, bom) {
        Ok(stamp) => Saved::Written(stamp),
        Err(err) => Saved::Failed(err.to_string()),
    }
}

/// What the disk says about an open tab's file.
#[derive(Debug)]
enum DiskState {
    /// As the tab last read or wrote it.
    Same,
    Gone,
    /// Changed under unsaved edits: the stamp on disk.
    Changed(FileStamp),
    /// Changed under a clean tab, and read again.
    Reread(Result<TextFile, ReadError>),
}

/// Checks `path` against the stamp its tab knows. Runs on the background executor.
fn check_disk(path: &Path, known: FileStamp, dirty: bool) -> DiskState {
    match fs::stamp(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => DiskState::Gone,
        // Unreadable for now (permissions, a save in progress): the next event checks again.
        Err(_) => DiskState::Same,
        Ok(stamp) if stamp == known => DiskState::Same,
        Ok(stamp) if dirty => DiskState::Changed(stamp),
        Ok(_) => DiskState::Reread(fs::read_text(path)),
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: GPUI's glob export includes a `test` macro that would replace the built-in one.
    use super::{DiskState, Saved, active_after_close, check_disk, write_file};
    use leaf_project::fs::{self, LineEnding};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("leaf-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn closing_a_tab_keeps_the_active_one_when_it_can() {
        assert_eq!(active_after_close(Some(2), 0, 3), Some(1));
        assert_eq!(active_after_close(Some(0), 2, 3), Some(0));
        assert_eq!(active_after_close(Some(1), 1, 2), Some(1));
        assert_eq!(active_after_close(Some(2), 2, 2), Some(1));
        assert_eq!(active_after_close(Some(0), 0, 0), None);
        assert_eq!(active_after_close(None, 0, 1), None);
    }

    #[test]
    fn write_file_asks_before_replacing_a_changed_file() {
        let dir = temp_dir("write-file");
        let path = dir.join("a.md");
        std::fs::write(&path, "one\n").unwrap();
        let read = fs::stamp(&path).unwrap();

        let written = match write_file(&path, "two\n", Some(read), LineEnding::Lf, false) {
            Saved::Written(stamp) => stamp,
            other => panic!("expected a write, got {other:?}"),
        };
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two\n");

        std::fs::write(&path, "changed elsewhere\n").unwrap();
        assert!(matches!(
            write_file(&path, "three\n", Some(written), LineEnding::Lf, false),
            Saved::ChangedOnDisk
        ));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "changed elsewhere\n"
        );

        // Overwriting, or writing a deleted file again, does not check.
        assert!(matches!(
            write_file(&path, "three\n", None, LineEnding::Lf, false),
            Saved::Written(_)
        ));
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(
            write_file(&path, "four\n", None, LineEnding::Lf, false),
            Saved::Written(_)
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "four\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn check_disk_follows_the_file() {
        let dir = temp_dir("check-disk");
        let path = dir.join("a.md");
        std::fs::write(&path, "one\n").unwrap();
        let known = fs::stamp(&path).unwrap();

        assert!(matches!(check_disk(&path, known, false), DiskState::Same));

        std::fs::write(&path, "one\ntwo\n").unwrap();
        assert!(matches!(
            check_disk(&path, known, true),
            DiskState::Changed(_)
        ));
        match check_disk(&path, known, false) {
            DiskState::Reread(Ok(file)) => assert_eq!(file.text, "one\ntwo\n"),
            other => panic!("expected a reread, got {other:?}"),
        }

        std::fs::remove_file(&path).unwrap();
        assert!(matches!(check_disk(&path, known, false), DiskState::Gone));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
