//! The folder tree on the left: the vein toward the selection, key hints, and IntelliJ moves.

use std::{
    ops::Range,
    path::{Path, PathBuf},
};

use gpui_kit::{
    component::{
        Icon,
        input::{Input, InputEvent, InputState},
    },
    prelude::FluentBuilder as _,
    *,
};
use leaf_project::{
    fs::{self, EntryKind, NameError},
    nav::{self, NavKey, SearchStep},
    tree::{Guide, ProjectTree, Row},
};

use crate::{
    keymap::{
        Collapse, Expand, FocusEditor, LeafThrough, NewFile, Open, PROJECT_PANEL, PageDown, PageUp,
        SelectFirst, SelectLast, SelectNext, SelectPrev,
    },
    theme::{self, c, size, z},
};

/// Row geometry at 100% zoom, from the design mockup: the chevron slot of a row at depth `d`
/// starts at `10 + 14d`, so a folder's chevron is centered on the guide its children draw.
const ROW_PADDING_LEFT: f32 = 10.;
const ROW_PADDING_RIGHT: f32 = 8.;
const ROW_GAP: f32 = 4.;
const CHEVRON_SLOT: f32 = 12.;
const CHEVRON_SIZE: f32 = 10.;

/// Tree width limits at 100% zoom, and the strip at its right edge that drags it.
const MIN_WIDTH: f32 = 180.;
const MAX_WIDTH: f32 = 480.;
const RESIZE_HANDLE: f32 = 4.;

/// Speed search field spacing at 100% zoom.
const SEARCH_MARGIN: f32 = 8.;
const SEARCH_PADDING: f32 = 6.;
/// The name input sits inside a row, one point clear of its top and bottom.
const EDIT_HEIGHT: f32 = size::TREE_ROW - 2.;

/// Key hint line spacing at 100% zoom.
const HINT_PADDING: f32 = 8.;
const HINT_GAP: f32 = 8.;
const HINT_KEY_GAP: f32 = 4.;
const KEY_CAP_PADDING: f32 = 3.;

/// The mockup's chevron. Inline bytes keep the tree independent of a registered icon bundle.
const CHEVRON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 10 10"><path d="M3.5 2 L7 5 L3.5 8" fill="none" stroke="black" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// Left padding of a row at `depth`.
fn row_padding(depth: usize) -> f32 {
    ROW_PADDING_LEFT + depth as f32 * size::TREE_INDENT
}

/// The x of the indent guide at `level`: under the chevron center of that level's folder.
fn guide_x(level: usize) -> f32 {
    row_padding(level) + CHEVRON_SLOT / 2.
}

fn clamp_width(width: f32) -> f32 {
    width.clamp(MIN_WIDTH, MAX_WIDTH)
}

/// The rows that fit whole in a list `list_height` tall, at least one so a page always moves.
fn rows_per_page(list_height: f32, row_height: f32) -> usize {
    ((list_height / row_height).floor() as usize).max(1)
}

/// `path` with the home folder written as `~`, the way a shell prompt shows it.
fn tilde(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

pub enum PanelEvent {
    /// Open a file. `leaf_through` keeps focus in the tree and reuses the leaf-through tab.
    Open { path: PathBuf, leaf_through: bool },
}

pub struct ProjectPanel {
    tree: ProjectTree,
    focus_handle: FocusHandle,
    scroll: UniformListScrollHandle,
    pub show_key_hints: bool,
    /// The tree width at 100% zoom, dragged from its right edge. The session keeps it.
    pub width: f32,
    /// The speed search text while a search is on (IntelliJ: type in the tree to find a name).
    search: Option<String>,
    /// Ends the search when focus leaves the tree. Made on first use: `new` has no window.
    focus_out: Option<Subscription>,
    /// The selection has no row yet (its folder is being read); scroll to it when it arrives.
    reveal_pending: bool,
    /// A name being typed into the tree (Cmd+N).
    edit: Option<Edit>,
}

/// A name being typed into the tree, in an input on its own line.
struct Edit {
    target: EditTarget,
    input: Entity<InputState>,
    /// Why the last Enter was refused, shown after the input.
    error: Option<SharedString>,
    /// The disk work for the last Enter has not come back yet.
    busy: bool,
    _subscription: Subscription,
}

#[derive(Clone)]
enum EditTarget {
    /// A new file, or a folder when the name ends in `/`, inside `dir`.
    New { dir: PathBuf },
}

impl EditTarget {
    fn hints(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            EditTarget::New { .. } => &[("↩", "Create"), ("Esc", "Cancel")],
        }
    }
}

/// One line of the list: a tree row, or the input for a new item right under its folder.
enum Item {
    Row(Row),
    New { depth: usize, guides: Vec<Guide> },
}

/// The input line and its refusal, as the list draws them.
#[derive(Clone)]
struct EditView {
    input: Entity<InputState>,
    error: Option<SharedString>,
}

/// What the right edge drags; the panel resizes as it moves.
struct ResizeDrag;

impl EventEmitter<PanelEvent> for ProjectPanel {}

impl Focusable for ProjectPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ProjectPanel {
    pub fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            tree: ProjectTree::new(root),
            focus_handle: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            show_key_hints: true,
            width: size::TREE_WIDTH,
            search: None,
            focus_out: None,
            reveal_pending: false,
            edit: None,
        };
        panel.load_dir(PathBuf::new(), cx);
        panel
    }

    pub fn root(&self) -> &Path {
        self.tree.root()
    }

    /// Reopens the folders and the selection a session saved; folders that no longer exist stay closed.
    pub fn restore(
        &mut self,
        expanded: &[PathBuf],
        selected: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        for dir in expanded {
            if self.tree.expand(dir) {
                self.load_dir(dir.clone(), cx);
            }
        }
        self.tree.select(selected);
        cx.notify();
    }

    pub fn expanded_dirs(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.tree.expanded_dirs().map(Path::to_path_buf).collect();
        dirs.sort();
        dirs
    }

    pub fn selected(&self) -> Option<&Path> {
        self.tree.selected()
    }

    /// Reads one folder off the main thread and shows it when it arrives.
    pub fn load_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let root = self.tree.root().to_path_buf();
        cx.spawn(async move |this, cx| {
            let listing = cx.background_spawn({
                let dir = dir.clone();
                async move { fs::list_dir(&root, &dir) }
            });
            let Ok(entries) = listing.await else { return };
            let _ = this.update(cx, |this, cx| {
                this.tree.set_listing(dir, entries);
                if this.reveal_pending {
                    this.reveal_selection();
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Selects `rel` (relative to the open folder) and opens the folders above it, reading the
    /// ones not read yet, then scrolls to it once its row is there. The workspace calls it to
    /// follow the active tab.
    pub fn reveal(&mut self, rel: &Path, cx: &mut Context<Self>) {
        for dir in self.tree.reveal(rel) {
            self.load_dir(dir, cx);
        }
        self.reveal_selection();
        cx.notify();
    }

    /// Folders changed on disk: re-read the ones on screen, forget the closed ones until they open.
    pub fn refresh_dirs(&mut self, dirs: &[PathBuf], cx: &mut Context<Self>) {
        for dir in dirs {
            if self.tree.listing(dir).is_none() {
                continue;
            }
            if dir.as_os_str().is_empty() || self.tree.is_expanded(dir) {
                self.load_dir(dir.clone(), cx);
            } else {
                self.tree.invalidate(dir);
            }
        }
    }

    fn apply(&mut self, key: NavKey, cx: &mut Context<Self>) {
        // The tree stays put while a name is typed: arrows that reach here were the input's.
        if self.edit.is_some() {
            return;
        }
        // Every move but Up and Down ends a speed search.
        self.search = None;
        let outcome = nav::navigate(&self.tree.rows(), self.tree.selected(), key);
        if let Some(dir) = outcome.expand
            && self.tree.expand(&dir)
        {
            self.load_dir(dir, cx);
        }
        if let Some(dir) = outcome.collapse {
            self.tree.collapse(&dir);
        }
        if let Some(path) = outcome.select {
            self.tree.select(Some(path));
        }
        self.reveal_selection();
        cx.notify();
    }

    /// Up and Down walk the matches while a speed search is on, the rows otherwise.
    fn up_down(&mut self, key: NavKey, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            return;
        }
        match (&self.search, key) {
            (Some(_), NavKey::Up) => self.search_step(SearchStep::Prev, cx),
            (Some(_), _) => self.search_step(SearchStep::Next, cx),
            (None, _) => self.apply(key, cx),
        }
    }

    /// Selects the match `step` points at; with no match the selection stays.
    fn search_step(&mut self, step: SearchStep, cx: &mut Context<Self>) {
        let Some(query) = &self.search else {
            return;
        };
        if let Some(path) = nav::speed_search(&self.tree.rows(), self.tree.selected(), query, step)
        {
            self.tree.select(Some(path));
            self.reveal_selection();
        }
        cx.notify();
    }

    /// Typing a name starts or extends a speed search; Backspace takes a character back.
    /// Keys with a binding (arrows, Enter, Space, Escape) arrive as actions instead.
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // While a name is being typed, the keys belong to its input.
        if self.edit.is_some() {
            return;
        }
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        // Option types special characters on macOS; leave it, like Cmd and Ctrl, to shortcuts.
        if modifiers.platform || modifiers.control || modifiers.alt || modifiers.function {
            return;
        }
        if keystroke.key == "backspace" {
            let Some(query) = &mut self.search else {
                return;
            };
            query.pop();
            if query.is_empty() {
                self.search = None;
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let Some(text) = keystroke
            .key_char
            .as_deref()
            .filter(|t| !t.is_empty() && !t.chars().any(char::is_control))
        else {
            return;
        };
        self.watch_focus(window, cx);
        self.search.get_or_insert_default().push_str(text);
        self.search_step(SearchStep::Here, cx);
        cx.stop_propagation();
    }

    /// Escape cancels a name being typed or clears a speed search; with neither, it goes on
    /// to focus the editor.
    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            self.end_edit(window, cx);
        } else if self.search.take().is_some() {
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    /// Cmd+N: an input for a new item under the selected folder, or the selected file's folder.
    fn new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            return;
        }
        self.search = None;
        let rows = self.tree.rows();
        let selected = self
            .tree
            .selected()
            .and_then(|s| rows.iter().find(|r| r.path == s));
        let dir = match selected {
            Some(row) if row.kind == EntryKind::Dir && !row.loops => row.path.clone(),
            Some(row) => row.path.parent().map(Path::to_path_buf).unwrap_or_default(),
            None => PathBuf::new(),
        };
        if !dir.as_os_str().is_empty() && self.tree.expand(&dir) {
            self.load_dir(dir.clone(), cx);
        }
        self.start_edit(EditTarget::New { dir }, "", 0..0, window, cx);
        if let Some(index) = self
            .items(self.tree.rows())
            .iter()
            .position(|item| matches!(item, Item::New { .. }))
        {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }

    /// Puts an input with `text` in the tree, `select` selected, and gives it focus.
    fn start_edit(
        &mut self,
        target: EditTarget,
        text: &str,
        select: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |state, cx| {
            state.set_value(text.to_string(), window, cx);
            state.set_selected_range(select, cx);
            state.focus(window, cx);
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.commit_edit(window, cx),
                InputEvent::Change => {
                    if let Some(edit) = &mut this.edit {
                        edit.error = None;
                        cx.notify();
                    }
                }
                _ => {}
            },
        );
        self.watch_focus(window, cx);
        self.edit = Some(Edit {
            target,
            input,
            error: None,
            busy: false,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Enter in the input: does the disk work off the main thread.
    fn commit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.edit else {
            return;
        };
        if edit.busy {
            return;
        }
        edit.busy = true;
        let typed = edit.input.read(cx).value().to_string();
        let root = self.tree.root().to_path_buf();
        match edit.target.clone() {
            EditTarget::New { dir } => {
                let work = cx.background_executor().spawn({
                    let dir = dir.clone();
                    async move { fs::create(&root, &dir, &typed) }
                });
                cx.spawn_in(window, async move |this, cx| {
                    let result = work.await;
                    let _ = this
                        .update_in(cx, |this, window, cx| this.created(dir, result, window, cx));
                })
                .detach();
            }
        }
    }

    fn created(
        &mut self,
        dir: PathBuf,
        result: Result<(PathBuf, EntryKind), NameError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (path, kind) = match result {
            Ok(created) => created,
            Err(err) => return self.refused(err, cx),
        };
        self.end_edit(window, cx);
        // The folder it went into changed; `reveal` reads the folders made on the way.
        self.load_dir(dir, cx);
        self.reveal(&path, cx);
        if kind == EntryKind::File {
            cx.emit(PanelEvent::Open {
                path,
                leaf_through: false,
            });
        }
    }

    /// Keeps the input open with the reason after it.
    fn refused(&mut self, err: NameError, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.edit {
            edit.busy = false;
            edit.error = Some(err.to_string().into());
        }
        cx.notify();
    }

    /// Drops the input and gives the tree its focus back.
    fn end_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.take().is_some() {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    /// The list lines: the rows, with the new-item input under its folder.
    fn items(&self, rows: Vec<Row>) -> Vec<Item> {
        let mut items: Vec<Item> = rows.into_iter().map(Item::Row).collect();
        let Some(EditTarget::New { dir }) = self.edit.as_ref().map(|e| &e.target) else {
            return items;
        };
        // Right under the folder's row, one level in. The open folder is no row: on top.
        let (at, depth) = items
            .iter()
            .enumerate()
            .find_map(|(i, item)| match item {
                Item::Row(row) if &row.path == dir => Some((i + 1, row.depth + 1)),
                _ => None,
            })
            .unwrap_or((0, 0));
        // The vein runs through the new line wherever it runs through the line below it.
        let below = match items.get(at) {
            Some(Item::Row(row)) => row.guides.as_slice(),
            _ => &[],
        };
        let guides = (0..depth)
            .map(|level| Guide {
                vein: below.get(level).is_some_and(|g| g.vein),
                tick: false,
            })
            .collect();
        items.insert(at, Item::New { depth, guides });
        items
    }

    fn watch_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_out.is_none() {
            self.focus_out = Some(
                cx.on_focus_out(&self.focus_handle, window, |this, _, _, cx| {
                    this.search = None;
                    this.edit = None;
                    cx.notify();
                }),
            );
        }
    }

    /// Scrolls just enough to show the selected row. A selection whose folder is still being
    /// read has no row yet; the scroll waits for the listing.
    fn reveal_selection(&mut self) {
        let selected = self.tree.selected();
        let index = self
            .items(self.tree.rows())
            .iter()
            .position(|item| matches!(item, Item::Row(r) if Some(r.path.as_path()) == selected));
        if let Some(index) = index {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        self.reveal_pending = index.is_none() && selected.is_some();
    }

    /// How many rows PageUp and PageDown move: as many as the list showed in the last frame.
    fn page_rows(&self, cx: &App) -> usize {
        let list_height = self
            .scroll
            .0
            .borrow()
            .last_item_size
            .map_or(0., |size| size.item.height.as_f32());
        rows_per_page(list_height, z(size::TREE_ROW, cx).as_f32())
    }

    fn open_selected(&mut self, leaf_through: bool, cx: &mut Context<Self>) {
        let Some(selected) = self.tree.selected().map(Path::to_path_buf) else {
            return;
        };
        let is_dir = self
            .tree
            .rows()
            .iter()
            .any(|r| r.path == selected && r.kind == EntryKind::Dir);
        if is_dir {
            if self.tree.is_expanded(&selected) {
                self.tree.collapse(&selected);
            } else if self.tree.expand(&selected) {
                self.load_dir(selected, cx);
            }
            self.reveal_selection();
            cx.notify();
        } else {
            cx.emit(PanelEvent::Open {
                path: selected,
                leaf_through,
            });
        }
    }

    /// A click selects; a folder toggles on the first click and a file opens on the second.
    /// Folders ignore the second click so that a double click does not undo itself.
    fn click_row(&mut self, path: PathBuf, kind: EntryKind, count: usize, cx: &mut Context<Self>) {
        self.tree.select(Some(path));
        let open = match kind {
            EntryKind::Dir => count == 1,
            EntryKind::File => count == 2,
        };
        if open {
            self.open_selected(false, cx);
        }
        self.reveal_selection();
        cx.notify();
    }

    /// A tree line at `depth`: height, indent and its guides with the vein. `is_dir` sets how far
    /// the vein's tick reaches into the line.
    fn row_frame(
        id: impl Into<ElementId>,
        depth: usize,
        guides: &[Guide],
        is_dir: bool,
        cx: &App,
    ) -> Stateful<Div> {
        let p = theme::colors(cx);
        let mut el = div()
            .id(id)
            .relative()
            .w_full()
            .flex()
            .items_center()
            .gap(z(ROW_GAP, cx))
            .h(z(size::TREE_ROW, cx))
            .pl(z(row_padding(depth), cx))
            .pr(z(ROW_PADDING_RIGHT, cx))
            .whitespace_nowrap();
        for (level, guide) in guides.iter().enumerate() {
            let x = guide_x(level);
            // The 2 px vein covers the 1 px guide and one pixel to its left.
            let (left, width, color) = if guide.vein {
                (x - 1., 2., p.accent)
            } else {
                (x, 1., p.guide)
            };
            el = el.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(z(left, cx))
                    .w(z(width, cx))
                    .bg(c(color)),
            );
            if guide.tick {
                // A folder's tick stops before its chevron; a file's runs through the empty slot.
                let tick = if is_dir { 7. } else { 19. };
                el = el.child(
                    div()
                        .absolute()
                        .top(z((size::TREE_ROW - 2.) / 2., cx))
                        .h(z(2., cx))
                        .left(z(left, cx))
                        .w(z(tick, cx))
                        .bg(c(color)),
                );
            }
        }
        el
    }

    /// `focused` is whether the tree has focus: the selection dims without it, the vein does not.
    /// The part of the name a speed search for `query` matched is set in medium weight.
    fn render_row(
        row: &Row,
        selected: bool,
        focused: bool,
        query: Option<&str>,
        cx: &App,
    ) -> Stateful<Div> {
        let p = theme::colors(cx);
        let is_dir = row.kind == EntryKind::Dir;
        let (ink, chevron_color) = match (selected, focused) {
            (true, true) => (p.sel_ink, p.sel_ink),
            (true, false) => (p.ink, p.sub),
            (false, _) if row.ignored => (p.ignored, p.sub),
            (false, _) => (p.ink, p.sub),
        };
        Self::row_frame(
            ElementId::Name(row.path.to_string_lossy().to_string().into()),
            row.depth,
            &row.guides,
            is_dir,
            cx,
        )
        .text_color(c(ink))
        .when(selected, |el| {
            el.bg(c(if focused { p.sel_bg } else { p.guide }))
        })
        .when(!selected, |el| el.hover(|style| style.bg(c(p.hover))))
        .child(
            div()
                .size(z(CHEVRON_SLOT, cx))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .when(is_dir, |slot| {
                    slot.child(
                        Icon::default()
                            .data(CHEVRON)
                            .size(z(CHEVRON_SIZE, cx))
                            .text_color(c(chevron_color))
                            .when(row.expanded, |icon| {
                                icon.rotate(radians(std::f32::consts::FRAC_PI_2))
                            }),
                    )
                }),
        )
        .child(name_label(&row.name, query))
    }

    /// The line where a new item's name is typed, under its folder.
    fn render_new(depth: usize, guides: &[Guide], edit: &EditView, cx: &App) -> Stateful<Div> {
        Self::row_frame("tree-new", depth, guides, false, cx)
            .child(div().size(z(CHEVRON_SLOT, cx)).flex_none())
            .child(edit_field(edit, cx))
    }
}

/// A row's name, its speed search match in medium weight.
fn name_label(name: &str, query: Option<&str>) -> StyledText {
    let text = StyledText::new(name.to_string());
    match query.and_then(|query| nav::name_match(name, query)) {
        Some(range) => text.with_highlights([(
            range,
            HighlightStyle {
                font_weight: Some(FontWeight::MEDIUM),
                ..Default::default()
            },
        )]),
        None => text,
    }
}

/// The name input of a tree line and, after it, why the last Enter was refused.
fn edit_field(edit: &EditView, cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(z(ROW_GAP, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h(z(EDIT_HEIGHT, cx))
                .overflow_hidden()
                .rounded(z(4., cx))
                .border_1()
                .border_color(c(p.line))
                .bg(c(p.paper))
                // A click inside stays there; the tree's own mouse-down would take the focus back.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    Input::new(&edit.input)
                        .appearance(false)
                        .size_full()
                        .px(z(SEARCH_PADDING, cx))
                        .py_0()
                        .text_size(z(size::UI_TEXT, cx))
                        .line_height(z(EDIT_HEIGHT - 2., cx)),
                ),
        )
        .when_some(edit.error.clone(), |el, error| {
            el.child(
                div()
                    .flex_none()
                    .text_size(z(12., cx))
                    .text_color(c(p.sub))
                    .child(error),
            )
        })
}

/// The line at the top of the list that shows what a speed search is looking for.
fn search_field(query: String, cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    div()
        .flex_none()
        .px(z(SEARCH_MARGIN, cx))
        .pb(z(SEARCH_MARGIN, cx))
        .child(
            div()
                .h(z(size::TREE_ROW, cx))
                .flex()
                .items_center()
                .px(z(SEARCH_PADDING, cx))
                .overflow_hidden()
                .whitespace_nowrap()
                .rounded(z(4., cx))
                .border_1()
                .border_color(c(p.line))
                .bg(c(p.paper))
                .text_color(c(p.ink))
                .child(query),
        )
}

impl Render for ProjectPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let focused = self.focus_handle.contains_focused(window, cx);
        let rows = self.tree.rows();
        let selected = self.tree.selected().map(Path::to_path_buf);
        let root = self.tree.root();
        let root_name = root.file_name().map_or_else(
            || root.display().to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let parent = root
            .parent()
            .map(|parent| tilde(parent, home.as_deref()))
            .unwrap_or_default();
        let selected_row = selected
            .as_ref()
            .and_then(|s| rows.iter().find(|r| &r.path == s))
            .cloned();
        let items = self.items(rows);
        let count = items.len();
        let hints = match &self.edit {
            Some(edit) => edit.target.hints(),
            None => hint_keys(selected_row.as_ref()),
        };
        div()
            .key_context(PROJECT_PANEL)
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.search = None;
                    this.edit = None;
                    this.focus_handle.focus(window, cx)
                }),
            )
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.up_down(NavKey::Up, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.up_down(NavKey::Down, cx)))
            .on_action(cx.listener(|this, _: &Expand, _, cx| this.apply(NavKey::Right, cx)))
            .on_action(cx.listener(|this, _: &Collapse, _, cx| this.apply(NavKey::Left, cx)))
            .on_action(cx.listener(|this, _: &SelectFirst, _, cx| this.apply(NavKey::First, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _, cx| this.apply(NavKey::Last, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                let page = this.page_rows(cx);
                this.apply(NavKey::PageUp(page), cx)
            }))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                let page = this.page_rows(cx);
                this.apply(NavKey::PageDown(page), cx)
            }))
            .on_action(cx.listener(|this, _: &Open, _, cx| {
                // Enter in a name input reaches here after the input took it.
                if this.edit.is_none() {
                    this.search = None;
                    this.open_selected(false, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &FocusEditor, window, cx| this.escape(window, cx)))
            .on_action(cx.listener(|this, _: &LeafThrough, _, cx| {
                if this.edit.is_some() {
                    // Space is bound in the tree; let it go on and type into the name.
                    cx.propagate();
                } else {
                    this.open_selected(true, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &NewFile, window, cx| this.new_file(window, cx)))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ResizeDrag>, _, cx| {
                    let width = (event.event.position.x - event.bounds.left()).as_f32();
                    this.width = clamp_width(width / theme::zoom(cx));
                    cx.notify();
                }),
            )
            .relative()
            // Clamped here too: the width may come back from a saved session.
            .w(z(clamp_width(self.width), cx))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(c(p.side))
            .border_r_1()
            .border_color(c(p.line))
            .text_size(z(size::UI_TEXT, cx))
            .child(
                div()
                    .h(z(size::TREE_HEAD, cx))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(z(8., cx))
                    .px(z(14., cx))
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(p.ink))
                            .child(root_name),
                    )
                    .child(
                        // A long parent path loses its start, keeping the folders nearest the root.
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis_start()
                            .text_size(z(12., cx))
                            .text_color(c(p.sub))
                            .child(parent),
                    ),
            )
            .when_some(self.search.clone(), |el, query| {
                el.child(search_field(query, cx))
            })
            .child(
                uniform_list("project-tree", count, {
                    let selected = selected.clone();
                    let query = self.search.clone();
                    let edit = self.edit.as_ref().map(|edit| EditView {
                        input: edit.input.clone(),
                        error: edit.error.clone(),
                    });
                    let panel = cx.weak_entity();
                    move |range, _, cx| {
                        items[range]
                            .iter()
                            .map(|item| match (item, &edit) {
                                (Item::Row(row), _) => {
                                    let (path, kind) = (row.path.clone(), row.kind);
                                    let panel = panel.clone();
                                    Self::render_row(
                                        row,
                                        selected.as_deref() == Some(row.path.as_path()),
                                        focused,
                                        query.as_deref(),
                                        cx,
                                    )
                                    .on_click(move |event, _, cx| {
                                        let count = event.click_count();
                                        let _ = panel.update(cx, |this, cx| {
                                            this.click_row(path.clone(), kind, count, cx)
                                        });
                                    })
                                    .into_any_element()
                                }
                                (Item::New { depth, guides }, Some(edit)) => {
                                    Self::render_new(*depth, guides, edit, cx).into_any_element()
                                }
                                (Item::New { .. }, None) => div().into_any_element(),
                            })
                            .collect()
                    }
                })
                .track_scroll(&self.scroll)
                .flex_1(),
            )
            .when(self.show_key_hints, |el| el.child(key_hints(hints, cx)))
            .child(
                // Inside the edge: the editor paints after the tree and would cover a handle outside it.
                div()
                    .id("tree-resize")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(z(RESIZE_HANDLE, cx))
                    .occlude()
                    .cursor_col_resize()
                    .on_drag(ResizeDrag, |_, _, _, cx| cx.new(|_| EmptyView)),
            )
    }
}

/// The keys the hint line offers for the selection, as (key, label).
fn hint_keys(selected: Option<&Row>) -> &'static [(&'static str, &'static str)] {
    match selected {
        Some(row) if row.kind == EntryKind::Dir && row.expanded => &[
            ("←", "Collapse"),
            ("→", "First child"),
            ("⌘⇧C", "Copy path"),
        ],
        // A folder that leads back up never opens, so offering Expand would be a lie.
        Some(row) if row.kind == EntryKind::Dir && row.loops => {
            &[("←", "Parent"), ("⌘⇧C", "Copy path")]
        }
        Some(row) if row.kind == EntryKind::Dir => {
            &[("→", "Expand"), ("←", "Parent"), ("⌘⇧C", "Copy path")]
        }
        _ => &[
            ("↩", "Open"),
            ("Space", "Leaf through"),
            ("⌘⇧C", "Copy path"),
        ],
    }
}

fn key_hints(keys: &'static [(&'static str, &'static str)], cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    // Tighter than the mockup (12 / 12 / 5 / 5): at the default width the file hints need every
    // point. A hint that still does not fit wraps onto a hidden second line instead of being cut.
    div()
        .h(z(size::KEY_HINTS, cx))
        .flex_none()
        .flex()
        .flex_wrap()
        .gap_x(z(HINT_GAP, cx))
        .px(z(HINT_PADDING, cx))
        .overflow_hidden()
        .whitespace_nowrap()
        .border_t_1()
        .border_color(c(p.line))
        .text_size(z(11.5, cx))
        .text_color(c(p.sub))
        .children(keys.iter().map(|&(key, label)| {
            div()
                .h_full()
                .flex()
                .items_center()
                .gap(z(HINT_KEY_GAP, cx))
                .child(
                    div()
                        .h(z(17., cx))
                        .flex()
                        .items_center()
                        .px(z(KEY_CAP_PADDING, cx))
                        .rounded(z(4., cx))
                        .border_1()
                        .border_color(c(p.line))
                        .bg(c(p.paper))
                        .text_color(c(p.ink))
                        .text_size(z(11., cx))
                        .child(key),
                )
                .child(label)
        }))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use leaf_project::{fs::EntryKind, tree::Row};

    use super::{CHEVRON_SLOT, clamp_width, guide_x, hint_keys, row_padding, rows_per_page, tilde};

    fn folder(expanded: bool, loops: bool) -> Row {
        Row {
            path: PathBuf::from("self"),
            name: "self".into(),
            depth: 0,
            kind: EntryKind::Dir,
            expanded,
            ignored: false,
            loops,
            guides: Vec::new(),
        }
    }

    fn labels(row: Option<&Row>) -> Vec<&'static str> {
        hint_keys(row).iter().map(|&(_, label)| label).collect()
    }

    #[test]
    fn a_folder_that_loops_offers_no_expand() {
        assert_eq!(
            labels(Some(&folder(false, false))),
            ["Expand", "Parent", "Copy path"]
        );
        assert_eq!(labels(Some(&folder(false, true))), ["Parent", "Copy path"]);
        assert_eq!(labels(None), ["Open", "Leaf through", "Copy path"]);
    }

    #[test]
    fn guides_follow_the_design_formula() {
        // The design puts the guide of depth L at x = 16 + 14L.
        for level in 0..5 {
            assert_eq!(guide_x(level), 16. + 14. * level as f32);
        }
    }

    #[test]
    fn a_folder_chevron_sits_on_the_guide_of_its_children() {
        for depth in 0..5 {
            assert_eq!(row_padding(depth) + CHEVRON_SLOT / 2., guide_x(depth));
        }
    }

    #[test]
    fn the_home_folder_shows_as_a_tilde() {
        let home = Some(Path::new("/Users/you"));
        assert_eq!(
            tilde(Path::new("/Users/you/Desktop/work"), home),
            "~/Desktop/work"
        );
        assert_eq!(tilde(Path::new("/Users/you"), home), "~");
        // Only whole folder names match: `/Users/younger` is not under `/Users/you`.
        assert_eq!(tilde(Path::new("/Users/younger"), home), "/Users/younger");
        assert_eq!(tilde(Path::new("/opt/work"), home), "/opt/work");
        assert_eq!(tilde(Path::new("/opt/work"), None), "/opt/work");
    }

    #[test]
    fn a_page_is_the_rows_that_fit_whole() {
        assert_eq!(rows_per_page(220., 22.), 10);
        assert_eq!(rows_per_page(230., 22.), 10);
        // Zoom scales both, so the count stays.
        assert_eq!(rows_per_page(220. * 1.5, 22. * 1.5), 10);
        // Before the first layout the list has no height; a page is still one row.
        assert_eq!(rows_per_page(0., 22.), 1);
    }

    #[test]
    fn the_tree_width_stays_between_its_limits() {
        assert_eq!(clamp_width(120.), 180.);
        assert_eq!(clamp_width(300.), 300.);
        assert_eq!(clamp_width(900.), 480.);
    }
}
