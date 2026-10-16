//! The folder tree on the left: the vein toward the selection, key hints, and IntelliJ moves.

use std::path::{Path, PathBuf};

use gpui_kit::{component::Icon, prelude::FluentBuilder as _, *};
use leaf_project::{
    fs::{self, EntryKind},
    nav::{self, NavKey},
    tree::{ProjectTree, Row},
};

use crate::{
    keymap::{
        Collapse, Expand, LeafThrough, Open, PROJECT_PANEL, PageDown, PageUp, SelectFirst,
        SelectLast, SelectNext, SelectPrev,
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
}

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
        };
        panel.load_dir(PathBuf::new(), cx);
        panel
    }

    pub fn root(&self) -> &Path {
        self.tree.root()
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

    /// Re-reads folders that changed on disk and are on screen.
    #[expect(dead_code, reason = "called by the folder watcher (Phase 1)")]
    pub fn refresh_dirs(&mut self, dirs: &[PathBuf], cx: &mut Context<Self>) {
        for dir in dirs {
            if self.tree.listing(dir).is_some() {
                self.load_dir(dir.clone(), cx);
            }
        }
    }

    fn apply(&mut self, key: NavKey, cx: &mut Context<Self>) {
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
    fn render_row(row: &Row, selected: bool, focused: bool, cx: &App) -> Stateful<Div> {
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
        .child(row.name.clone())
    }
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
                    this.focus_handle.focus(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.apply(NavKey::Up, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.apply(NavKey::Down, cx)))
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
            .on_action(cx.listener(|this, _: &Open, _, cx| this.open_selected(false, cx)))
            .on_action(cx.listener(|this, _: &LeafThrough, _, cx| this.open_selected(true, cx)))
            .w(z(size::TREE_WIDTH, cx))
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
            .child(
                uniform_list("project-tree", count, {
                    let rows = rows.clone();
                    let selected = selected.clone();
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
    }
}

fn key_hints(selected: Option<&Row>, cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    let keys: [(&str, &str); 3] = match selected {
        Some(row) if row.kind == EntryKind::Dir && row.expanded => [
            ("←", "Collapse"),
            ("→", "First child"),
            ("⌘⇧C", "Copy path"),
        ],
        Some(row) if row.kind == EntryKind::Dir => {
            [("→", "Expand"), ("←", "Parent"), ("⌘⇧C", "Copy path")]
        }
        _ => [
            ("↩", "Open"),
            ("Space", "Leaf through"),
            ("⌘⇧C", "Copy path"),
        ],
    };
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
        .children(keys.into_iter().map(|(key, label)| {
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
    use std::path::Path;

    use super::{CHEVRON_SLOT, guide_x, row_padding, rows_per_page, tilde};

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
}
