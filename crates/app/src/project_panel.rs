//! The folder tree on the left: the vein toward the selection, key hints, and IntelliJ moves.

use std::path::{Path, PathBuf};

use gpui_kit::{prelude::FluentBuilder as _, *};
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
        let rows = self.tree.rows();
        let outcome = nav::navigate(&rows, self.tree.selected(), key);
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
            if let Some(index) = self
                .tree
                .rows()
                .iter()
                .position(|r| Some(r.path.as_path()) == self.tree.selected())
            {
                self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
            }
        }
        cx.notify();
    }

    fn page(&self, cx: &App) -> usize {
        let _ = cx;
        20
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
            cx.notify();
        } else {
            cx.emit(PanelEvent::Open {
                path: selected,
                leaf_through,
            });
        }
    }

    fn render_row(row: &Row, selected: bool, cx: &App) -> Stateful<Div> {
        let p = theme::colors(cx);
        let indent = size::TREE_INDENT;
        let mut el = div()
            .id(ElementId::Name(
                row.path.to_string_lossy().to_string().into(),
            ))
            .relative()
            .flex()
            .items_center()
            .h(z(size::TREE_ROW, cx))
            .pl(z(10. + row.depth as f32 * indent, cx))
            .text_color(c(if selected {
                p.sel_ink
            } else if row.ignored {
                p.ignored
            } else {
                p.ink
            }))
            .when(selected, |el| el.bg(c(p.sel_bg)));
        for (level, guide) in row.guides.iter().enumerate() {
            let x = 16. + level as f32 * indent;
            let color = c(if guide.vein { p.accent } else { p.guide });
            el = el.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(z(if guide.vein { x - 1. } else { x }, cx))
                    .w(z(if guide.vein { 2. } else { 1. }, cx))
                    .bg(color),
            );
            if guide.tick {
                let width = if row.kind == EntryKind::Dir { 7. } else { 19. };
                el = el.child(
                    div()
                        .absolute()
                        .top(z(10., cx))
                        .h(z(2., cx))
                        .left(z(x - 1., cx))
                        .w(z(width, cx))
                        .bg(color),
                );
            }
        }
        let chevron = match (row.kind, row.expanded) {
            (EntryKind::Dir, true) => "▾",
            (EntryKind::Dir, false) => "▸",
            (EntryKind::File, _) => "",
        };
        el.child(
            div()
                .w(z(12., cx))
                .flex_none()
                .text_color(c(if selected { p.sel_ink } else { p.sub }))
                .child(chevron),
        )
        .child(div().ml_1().child(row.name.clone()))
    }
}

impl Render for ProjectPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let rows = self.tree.rows();
        let selected = self.tree.selected().map(Path::to_path_buf);
        let root_name = self
            .tree
            .root()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let parent = self
            .tree
            .root()
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let page = self.page(cx);
        let count = rows.len();
        let selected_row = selected
            .as_ref()
            .and_then(|s| rows.iter().find(|r| &r.path == s))
            .cloned();
        div()
            .key_context(PROJECT_PANEL)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.apply(NavKey::Up, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.apply(NavKey::Down, cx)))
            .on_action(cx.listener(|this, _: &Expand, _, cx| this.apply(NavKey::Right, cx)))
            .on_action(cx.listener(|this, _: &Collapse, _, cx| this.apply(NavKey::Left, cx)))
            .on_action(cx.listener(|this, _: &SelectFirst, _, cx| this.apply(NavKey::First, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _, cx| this.apply(NavKey::Last, cx)))
            .on_action(
                cx.listener(move |this, _: &PageUp, _, cx| this.apply(NavKey::PageUp(page), cx)),
            )
            .on_action(
                cx.listener(move |this, _: &PageDown, _, cx| {
                    this.apply(NavKey::PageDown(page), cx)
                }),
            )
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
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3p5()
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(c(p.ink))
                            .child(root_name),
                    )
                    .child(
                        div()
                            .text_size(z(12., cx))
                            .text_color(c(p.sub))
                            .child(parent),
                    ),
            )
            .child(
                uniform_list("project-tree", count, {
                    let rows = rows.clone();
                    let selected = selected.clone();
                    move |range, _, cx| {
                        rows[range]
                            .iter()
                            .map(|row| {
                                Self::render_row(
                                    row,
                                    selected.as_deref() == Some(row.path.as_path()),
                                    cx,
                                )
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
    div()
        .h(z(size::KEY_HINTS, cx))
        .flex_none()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .border_t_1()
        .border_color(c(p.line))
        .text_size(z(11.5, cx))
        .text_color(c(p.sub))
        .children(keys.into_iter().map(|(key, label)| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .px_1()
                        .rounded(px(4.))
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
