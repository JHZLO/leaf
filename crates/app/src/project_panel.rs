//! The folder tree on the left: the vein toward the selection, key hints, and IntelliJ moves.

use std::path::{Path, PathBuf};

use gpui_kit::{component::Icon, prelude::FluentBuilder as _, *};
use leaf_project::{
    fs::{self, EntryKind},
    nav::{self, NavKey, SearchStep},
    tree::{ProjectTree, Row},
};

use crate::{
    keymap::{
        Collapse, Expand, FocusEditor, LeafThrough, Open, PROJECT_PANEL, PageDown, PageUp,
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
                cx.notify();
            });
        })
        .detach();
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

    /// Escape clears a speed search first; with none, it goes on to focus the editor.
    fn escape(&mut self, cx: &mut Context<Self>) {
        if self.search.take().is_some() {
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    fn watch_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_out.is_none() {
            self.focus_out = Some(
                cx.on_focus_out(&self.focus_handle, window, |this, _, _, cx| {
                    this.search = None;
                    cx.notify();
                }),
            );
        }
    }

    /// Scrolls just enough to show the selected row.
    fn reveal_selection(&self) {
        let selected = self.tree.selected();
        if let Some(index) = self
            .tree
            .rows()
            .iter()
            .position(|r| Some(r.path.as_path()) == selected)
        {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
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
        let mut el = div()
            .id(ElementId::Name(
                row.path.to_string_lossy().to_string().into(),
            ))
            .relative()
            .w_full()
            .flex()
            .items_center()
            .gap(z(ROW_GAP, cx))
            .h(z(size::TREE_ROW, cx))
            .pl(z(row_padding(row.depth), cx))
            .pr(z(ROW_PADDING_RIGHT, cx))
            .whitespace_nowrap()
            .text_color(c(ink))
            .when(selected, |el| {
                el.bg(c(if focused { p.sel_bg } else { p.guide }))
            })
            .when(!selected, |el| el.hover(|style| style.bg(c(p.hover))));
        for (level, guide) in row.guides.iter().enumerate() {
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
        el.child(
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
        let count = rows.len();
        let selected_row = selected
            .as_ref()
            .and_then(|s| rows.iter().find(|r| &r.path == s))
            .cloned();
        div()
            .key_context(PROJECT_PANEL)
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.search = None;
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
                this.search = None;
                this.open_selected(false, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusEditor, _, cx| this.escape(cx)))
            .on_action(cx.listener(|this, _: &LeafThrough, _, cx| this.open_selected(true, cx)))
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
                    let rows = rows.clone();
                    let selected = selected.clone();
                    let query = self.search.clone();
                    let panel = cx.weak_entity();
                    move |range, _, cx| {
                        rows[range]
                            .iter()
                            .map(|row| {
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
                            })
                            .collect()
                    }
                })
                .track_scroll(&self.scroll)
                .flex_1(),
            )
            .when(self.show_key_hints, |el| {
                el.child(key_hints(selected_row.as_ref(), cx))
            })
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

fn key_hints(selected: Option<&Row>, cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    let keys = hint_keys(selected);
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
