//! The folder tree on the left: the route to the selection, key hints, and IntelliJ moves.

use std::{
    collections::VecDeque,
    ops::Range,
    path::{Path, PathBuf},
    time::Instant,
};

use gpui_kit::{
    assets::IconName,
    component::{
        Icon,
        input::{Input, InputEvent, InputState},
        tooltip::Tooltip,
    },
    prelude::FluentBuilder as _,
    *,
};
use leaf_project::{
    folders::Folders,
    fs::{self, EntryKind, NameError},
    nav::{self, NavKey, SearchStep},
    tree::{self, ProjectTree, Row},
};

use crate::{
    icons,
    keymap::{
        Collapse, CopyPath, Expand, FocusEditor, LeafThrough, NewFile, Open, PROJECT_PANEL,
        PageDown, PageUp, Rename, RevealInFinder, SelectFirst, SelectLast, SelectNext, SelectPrev,
        Trash,
    },
    route,
    theme::{self, c, size, z},
};

/// Row geometry at 100% zoom, from the design mockup: the chevron slot of a row at depth `d`
/// starts at `10 + 14d`, so a folder's chevron is centered on the guide its children draw.
const ROW_PADDING_LEFT: f32 = 10.;
const ROW_PADDING_RIGHT: f32 = 8.;
const ROW_GAP: f32 = 4.;
const CHEVRON_SLOT: f32 = 12.;
const CHEVRON_SIZE: f32 = 10.;
/// The file type icon after the chevron.
const ICON_SIZE: f32 = 14.;

/// Tree width limits at 100% zoom, and the strip at its right edge that drags it.
const MIN_WIDTH: f32 = 180.;
const MAX_WIDTH: f32 = 480.;

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

/// The x of the column at `level`, where the route runs: under the chevron center of that
/// level's folders.
fn column_x(level: usize) -> f32 {
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
    /// The header's locate button: select the active tab's file (IntelliJ's Select Opened File).
    Locate,
    /// The header's minus: hide the tree, as Cmd+1 in it does.
    Hide,
    /// The more menu's Add Folder…: ask for a folder to show in this window too.
    AddFolder,
    /// A folder row's Remove from Window (relative path of the folder's row).
    RemoveFolder(PathBuf),
}

/// A small list floating over the tree.
enum Menu {
    /// Cmd+N or the header's plus, as IntelliJ's New popup: whether to make a file or a folder in
    /// `dir`. `choice` is the highlighted line (0 File, 1 Directory).
    New { dir: PathBuf, choice: usize },
    /// The header's more button.
    Options,
    /// Right-click on one of several folders' rows.
    Folder { path: PathBuf },
}

/// What Expand All reads at most, so a huge folder does not flood the tree.
const EXPAND_ALL_MOST: usize = 300;

/// Items the tree changed on disk, so open tabs can follow. Paths are relative to the open
/// folder; a folder's event covers everything inside it.
pub enum FileEvent {
    Renamed { from: PathBuf, to: PathBuf },
    Trashed { path: PathBuf },
}

pub struct ProjectPanel {
    tree: ProjectTree,
    /// Where the tree's relative paths are on disk.
    folders: Folders,
    focus_handle: FocusHandle,
    scroll: UniformListScrollHandle,
    pub show_key_hints: bool,
    /// The tree width at 100% zoom, dragged from its right edge. The session keeps it.
    pub width: f32,
    /// The speed search text while a search is on (IntelliJ: type in the tree to find a name).
    search: Option<String>,
    /// Ends the search and drops a name being typed when focus leaves the tree. Made on first
    /// use: `new` has no window.
    focus_out: Option<Subscription>,
    /// The selection has no row yet (its folder is being read); scroll to it when it arrives.
    reveal_pending: bool,
    /// A name being typed into the tree (Cmd+N, Shift+F6).
    edit: Option<Edit>,
    menu: Option<Menu>,
    /// The route to the selection, gliding when the selection moves.
    route: route::Motion,
    /// The row under the pointer: it gets a background and a dashed way to it off the route.
    hovered: Option<PathBuf>,
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
    /// A new file, or a folder, inside `dir`. A name ending in `/` is a folder either way.
    New { dir: PathBuf, kind: EntryKind },
    /// A new name for the item at `path`.
    Rename { path: PathBuf },
}

impl EditTarget {
    fn hints(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            EditTarget::New { .. } => &[("↩", "Create"), ("Esc", "Cancel")],
            EditTarget::Rename { .. } => &[("↩", "Rename"), ("Esc", "Cancel")],
        }
    }
}

/// What a rename input selects at first: the name up to its extension, so typing replaces
/// the name and keeps `.md`. A folder, or a name with no extension (`.gitignore`), is selected whole.
fn name_selection(name: &str, kind: EntryKind) -> Range<usize> {
    match name.rfind('.') {
        Some(dot) if dot > 0 && kind == EntryKind::File => 0..dot,
        _ => 0..name.len(),
    }
}

/// One line of the list: a tree row, or the input for a new item right under its folder.
enum Item {
    Row(Row),
    New { depth: usize },
}

/// The input line and its refusal, as the list draws them.
struct EditView {
    input: Entity<InputState>,
    error: Option<SharedString>,
}

/// What the gap right of the tree drags; the panel resizes as it moves.
pub struct ResizeDrag;

impl EventEmitter<PanelEvent> for ProjectPanel {}
impl EventEmitter<FileEvent> for ProjectPanel {}

impl Focusable for ProjectPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ProjectPanel {
    pub fn new(folders: Folders, cx: &mut Context<Self>) -> Self {
        let mut tree = ProjectTree::new(folders.first().to_path_buf());
        // Several folders sit side by side at the top, as the user asked (not under a node).
        if folders.several() {
            tree.hide_root();
        }
        let mut panel = Self {
            tree,
            folders,
            focus_handle: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            show_key_hints: crate::settings(cx).key_hints,
            width: size::TREE_WIDTH,
            search: None,
            focus_out: None,
            reveal_pending: false,
            edit: None,
            menu: None,
            route: route::Motion::default(),
            hovered: None,
        };
        panel.load_dir(PathBuf::new(), cx);
        panel
    }

    pub fn folders(&self) -> &Folders {
        &self.folders
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

    /// The rows the tree shows, top to bottom; for the UI checks.
    pub fn row_paths(&self) -> Vec<PathBuf> {
        self.tree.rows().into_iter().map(|row| row.path).collect()
    }

    /// Reads one folder off the main thread and shows it when it arrives.
    pub fn load_dir(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        let folders = self.folders.clone();
        cx.spawn(async move |this, cx| {
            let listing = cx.background_spawn({
                let dir = dir.clone();
                async move { list(&folders, &dir) }
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
        if let Some(Menu::New { choice, .. }) = &mut self.menu {
            *choice = if key == NavKey::Up { 0 } else { 1 };
            cx.notify();
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
        // F and D pick in the New popup, as the first letters of its lines.
        if matches!(self.menu, Some(Menu::New { .. })) {
            match keystroke.key.as_str() {
                "f" => self.choose_new(EntryKind::File, window, cx),
                "d" => self.choose_new(EntryKind::Dir, window, cx),
                _ => {}
            }
            cx.stop_propagation();
            return;
        }
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
        if self.menu.take().is_some() {
            cx.notify();
        } else if self.edit.is_some() {
            self.end_edit(window, cx);
        } else if self.search.take().is_some() {
            cx.notify();
        } else {
            cx.propagate();
        }
    }

    /// Cmd+N: asks whether to make a file or a folder (IntelliJ's New popup), for the selected
    /// folder or the selected file's folder.
    fn new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            return;
        }
        self.watch_focus(window, cx);
        self.search = None;
        let rows = self.tree.rows();
        let selected = self
            .tree
            .selected()
            .and_then(|s| rows.iter().find(|r| r.path == s));
        let mut dir = match selected {
            Some(row) if row.kind == EntryKind::Dir && !row.loops => row.path.clone(),
            Some(row) => row.path.parent().map(Path::to_path_buf).unwrap_or_default(),
            None => PathBuf::new(),
        };
        // The window's node of several folders is on no disk: the first folder takes the item.
        if self.folders.mount(&dir).is_none() {
            dir = PathBuf::from(&self.folders.keys()[0]);
        }
        if self.tree.expand(&dir) {
            self.load_dir(dir.clone(), cx);
        }
        self.menu = Some(Menu::New { dir, choice: 0 });
        self.reveal_selection();
        cx.notify();
    }

    /// Right-click on one of several folders' rows: selects it and shows its menu.
    fn open_folder_menu(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        self.watch_focus(window, cx);
        self.tree.select(Some(path.clone()));
        self.menu = Some(Menu::Folder { path });
        cx.notify();
    }

    /// The New popup's choice: an input for the new item's name, right under its folder.
    fn choose_new(&mut self, kind: EntryKind, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Menu::New { dir, .. }) = self.menu.take() else {
            return;
        };
        self.start_edit(EditTarget::New { dir, kind }, "", 0..0, window, cx);
        if let Some(index) = self
            .items(self.tree.rows())
            .iter()
            .position(|item| matches!(item, Item::New { .. }))
        {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
    }

    /// The header's Expand All: opens every folder that is not gitignored, reading them off the main
    /// thread, up to [`EXPAND_ALL_MOST`] folders.
    fn expand_all(&mut self, cx: &mut Context<Self>) {
        let folders = self.folders.clone();
        let read = cx.background_spawn(async move {
            let mut queue = VecDeque::from([PathBuf::new()]);
            let mut found = Vec::new();
            while let Some(dir) = queue.pop_front() {
                if found.len() >= EXPAND_ALL_MOST {
                    break;
                }
                let Ok(entries) = list(&folders, &dir) else {
                    continue;
                };
                queue.extend(
                    entries
                        .iter()
                        .filter(|e| e.kind == EntryKind::Dir && !e.ignored && !e.loops)
                        .map(|e| dir.join(&e.name)),
                );
                found.push((dir, entries));
            }
            found
        });
        cx.spawn(async move |this, cx| {
            let found = read.await;
            let _ = this.update(cx, |this, cx| {
                for (dir, entries) in found {
                    this.tree.set_listing(dir.clone(), entries);
                    this.tree.expand(&dir);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// IntelliJ's tool window header: the title, then New, Select Opened File, Expand All,
    /// Collapse All, more options and Hide.
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = theme::colors(cx);
        let options = matches!(self.menu, Some(Menu::Options)).then(|| self.options_popup(cx));
        div()
            .h(z(size::TREE_HEAD, cx))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .pl(z(14., cx))
            .pr(z(6., cx))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(c(p.ink))
                    .child("Project"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(z(2., cx))
                    .child(
                        // On mouse down, so the tree's own mouse down does not close the popup at once.
                        header_button("tree-new", IconName::Plus, "New…", cx).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.focus_handle.focus(window, cx);
                                this.new_file(window, cx);
                            }),
                        ),
                    )
                    .child(
                        header_button(
                            "tree-locate",
                            IconName::LocateFixed,
                            "Select Opened File",
                            cx,
                        )
                        .on_click(
                            cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PanelEvent::Locate)),
                        ),
                    )
                    .child(
                        header_button("tree-expand", IconName::ChevronsUpDown, "Expand All", cx)
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.expand_all(cx)),
                            ),
                    )
                    .child(
                        header_button(
                            "tree-collapse",
                            IconName::ChevronsDownUp,
                            "Collapse All",
                            cx,
                        )
                        .on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.tree.collapse_all();
                                this.reveal_selection();
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        div()
                            .relative()
                            .child(
                                header_button(
                                    "tree-options",
                                    IconName::EllipsisVertical,
                                    "Options",
                                    cx,
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _: &MouseDownEvent, window, cx| {
                                        cx.stop_propagation();
                                        this.focus_handle.focus(window, cx);
                                        this.watch_focus(window, cx);
                                        this.menu = match this.menu {
                                            Some(Menu::Options) => None,
                                            _ => Some(Menu::Options),
                                        };
                                        cx.notify();
                                    }),
                                ),
                            )
                            .children(options),
                    )
                    .child(
                        header_button("tree-hide", IconName::Minus, "Hide", cx).on_click(
                            cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PanelEvent::Hide)),
                        ),
                    ),
            )
    }

    /// The more button's list: the key hint line, and the copy and reveal actions for the selection.
    fn options_popup(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let hints = self.show_key_hints;
        floating(
            HEADER_BUTTON + 2.,
            Anchor::TopRight,
            popup_box(cx)
                .child(
                    popup_item(
                        "tree-option-add",
                        Some(IconName::FolderPlus),
                        "Add Folder…",
                        false,
                        cx,
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.menu = None;
                        cx.emit(PanelEvent::AddFolder);
                        cx.notify();
                    })),
                )
                .child(
                    popup_item(
                        "tree-option-hints",
                        hints.then_some(IconName::Check),
                        "Key Hints",
                        false,
                        cx,
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.menu = None;
                        this.show_key_hints = !this.show_key_hints;
                        cx.notify();
                    })),
                )
                .child(
                    popup_item("tree-option-copy", None, "Copy Path", false, cx).on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.menu = None;
                            window.dispatch_action(Box::new(CopyPath), cx);
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    popup_item("tree-option-reveal", None, "Reveal in Finder", false, cx).on_click(
                        cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.menu = None;
                            window.dispatch_action(Box::new(RevealInFinder), cx);
                            cx.notify();
                        }),
                    ),
                ),
            cx,
        )
    }

    /// Shift+F6: the selected row's name becomes an input.
    fn rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            return;
        }
        let rows = self.tree.rows();
        // A folder's own row renames nothing: the window is that folder.
        let Some(row) = self
            .tree
            .selected()
            .filter(|s| !s.as_os_str().is_empty() && !self.folders.is_folder(s))
            .and_then(|s| rows.iter().find(|r| r.path == s))
        else {
            return;
        };
        self.search = None;
        let target = EditTarget::Rename {
            path: row.path.clone(),
        };
        let select = name_selection(&row.name, row.kind);
        self.start_edit(target, &row.name, select, window, cx);
        self.reveal_selection();
    }

    /// Cmd+Backspace: asks, then moves the selection to the Trash off the main thread.
    fn trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit.is_some() {
            return;
        }
        // Never a folder of the window itself (several leave by Remove from Window).
        let Some(path) = self
            .tree
            .selected()
            .filter(|s| !s.as_os_str().is_empty() && !self.folders.is_folder(s))
            .map(Path::to_path_buf)
        else {
            return;
        };
        self.search = None;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Move “{name}” to the Trash?"),
            None,
            &["Move to Trash", "Cancel"],
            cx,
        );
        let Some((root, rest, _)) = self.folders.mount(&path) else {
            return;
        };
        let root = root.to_path_buf();
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let work = cx
                .background_executor()
                .spawn(async move { fs::trash(&root, &rest) });
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    this.tree.forget(&path);
                    // Reading the folder again moves the selection to a neighbor.
                    this.load_dir(path.parent().map(Path::to_path_buf).unwrap_or_default(), cx);
                    cx.emit(FileEvent::Trashed { path });
                }
                Err(err) => {
                    // Only a report: there is no answer to wait for.
                    drop(window.prompt(
                        PromptLevel::Critical,
                        &format!("“{name}” could not be moved to the Trash"),
                        Some(&err.to_string()),
                        &["OK"],
                        cx,
                    ));
                }
            });
        })
        .detach();
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
        let target = match &edit.target {
            EditTarget::New { dir, .. } => dir,
            EditTarget::Rename { path } => path,
        };
        let Some((root, rest, prefix)) = self.folders.mount(target) else {
            return;
        };
        let root = root.to_path_buf();
        match edit.target.clone() {
            EditTarget::New { dir, kind } => {
                let typed = match kind {
                    EntryKind::Dir if !typed.trim_end().ends_with('/') => format!("{typed}/"),
                    _ => typed,
                };
                // Made inside its folder on disk, then named as the tree names it.
                let work = cx.background_executor().spawn(async move {
                    fs::create(&root, &rest, &typed).map(|(made, kind)| (prefix.join(made), kind))
                });
                cx.spawn_in(window, async move |this, cx| {
                    let result = work.await;
                    let _ = this
                        .update_in(cx, |this, window, cx| this.created(dir, result, window, cx));
                })
                .detach();
            }
            EditTarget::Rename { path } => {
                let work = cx.background_executor().spawn(async move {
                    fs::rename(&root, &rest, &typed).map(|renamed| prefix.join(renamed))
                });
                cx.spawn_in(window, async move |this, cx| {
                    let result = work.await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        this.renamed(path, result, window, cx)
                    });
                })
                .detach();
            }
        }
    }

    fn renamed(
        &mut self,
        from: PathBuf,
        result: Result<PathBuf, NameError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = match result {
            Ok(to) => to,
            Err(err) => return self.refused(err, cx),
        };
        self.end_edit(window, cx);
        if to != from {
            self.tree.renamed(&from, &to);
            self.load_dir(from.parent().map(Path::to_path_buf).unwrap_or_default(), cx);
            cx.emit(FileEvent::Renamed { from, to });
        }
        self.reveal_selection();
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
        let Some(EditTarget::New { dir, .. }) = self.edit.as_ref().map(|e| &e.target) else {
            return items;
        };
        // Right under the folder's row, one level in.
        let (at, depth) = items
            .iter()
            .enumerate()
            .find_map(|(i, item)| match item {
                Item::Row(row) if &row.path == dir => Some((i + 1, row.depth + 1)),
                _ => None,
            })
            .unwrap_or((0, 0));
        items.insert(at, Item::New { depth });
        items
    }

    fn watch_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_out.is_none() {
            self.focus_out = Some(
                cx.on_focus_out(&self.focus_handle, window, |this, _, _, cx| {
                    this.search = None;
                    this.edit = None;
                    this.menu = None;
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

    /// The pointer entered or left the row at `path`.
    fn hover_row(&mut self, path: &Path, hovered: bool, cx: &mut Context<Self>) {
        if hovered && self.hovered.as_deref() != Some(path) {
            self.hovered = Some(path.to_path_buf());
            cx.notify();
        } else if !hovered && self.hovered.as_deref() == Some(path) {
            self.hovered = None;
            cx.notify();
        }
    }

    /// A tree line at `depth`: its height and indent.
    fn row_frame(id: impl Into<ElementId>, depth: usize, cx: &App) -> Stateful<Div> {
        div()
            .id(id)
            .relative()
            .w_full()
            .flex()
            .items_center()
            .gap(z(ROW_GAP, cx))
            .h(z(size::TREE_ROW, cx))
            .pl(z(row_padding(depth), cx))
            .pr(z(ROW_PADDING_RIGHT, cx))
            .whitespace_nowrap()
    }

    /// `selection` is `Some(focused)` on the selected row, whose ink dims when the tree has no
    /// focus. `route` is the color of the route's line at a row on it: the row's chevron gives way
    /// to the route's ring, its icon takes the line's color and its name is set in medium. The
    /// backgrounds and the route itself are painted under the rows (see `route`).
    /// The part of the name a speed search for `query` matched is set in semibold.
    /// With `rename`, the name is an input. A folder of the window (and the window's own node) is
    /// set in medium and shows `hint`, where it is on disk.
    fn render_row(
        row: &Row,
        selection: Option<bool>,
        route: Option<u32>,
        query: Option<&str>,
        rename: Option<&EditView>,
        hint: Option<&SharedString>,
        cx: &App,
    ) -> Stateful<Div> {
        let p = theme::colors(cx);
        let is_dir = row.kind == EntryKind::Dir;
        let folder_row = row.path.as_os_str().is_empty() || hint.is_some();
        let (ink, chevron_color) = match selection {
            Some(true) => (p.sel_ink, p.sel_ink),
            Some(false) => (p.ink, p.sub),
            None if row.ignored => (p.ignored, p.sub),
            None => (p.ink, p.sub),
        };
        Self::row_frame(
            ElementId::Name(row.path.to_string_lossy().to_string().into()),
            row.depth,
            cx,
        )
        .text_color(c(ink))
        .when(route.is_some() && !folder_row, |el| {
            el.font_weight(FontWeight::MEDIUM)
        })
        .child(
            div()
                .size(z(CHEVRON_SLOT, cx))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .when(is_dir && route.is_none(), |slot| {
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
        .child({
            let (icon, tint) = if is_dir {
                icons::folder_icon(row.expanded, folder_row)
            } else {
                icons::file_icon(&row.name)
            };
            let color = match route {
                Some(line) => line,
                None if row.ignored => p.ignored,
                None => tint.color(p),
            };
            Icon::new(icon)
                .size(z(ICON_SIZE, cx))
                .flex_none()
                .text_color(c(color))
        })
        .map(|el| match rename {
            Some(edit) => el.child(edit_field(edit, cx)),
            None if folder_row => el
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::MEDIUM)
                        .child(name_label(&row.name, query)),
                )
                .child(
                    // A long path loses its start, keeping the folders nearest the name.
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis_start()
                        .text_size(z(12., cx))
                        .text_color(c(p.sub))
                        .children(hint.cloned()),
                ),
            None => el.child(name_label(&row.name, query)),
        })
    }

    /// The line where a new item's name is typed, under its folder.
    fn render_new(depth: usize, edit: &EditView, cx: &App) -> Stateful<Div> {
        Self::row_frame("tree-new", depth, cx)
            .child(div().size(z(CHEVRON_SLOT, cx)).flex_none())
            .child(edit_field(edit, cx))
    }
}

/// A folder's listing. With several folders the window's node lists them.
fn list(folders: &Folders, dir: &Path) -> std::io::Result<Vec<leaf_project::fs::DirEntry>> {
    if folders.several() && dir.as_os_str().is_empty() {
        return Ok(folders.entries());
    }
    match folders.mount(dir) {
        Some((root, rest, _)) => fs::list_dir(root, &rest),
        None => Err(std::io::ErrorKind::NotFound.into()),
    }
}

/// The header's icon buttons: square, the icon in `sub`, a hover fill, a tooltip.
const HEADER_BUTTON: f32 = 22.;

fn header_button(
    id: &'static str,
    icon: IconName,
    tooltip: &'static str,
    cx: &App,
) -> Stateful<Div> {
    let p = theme::colors(cx);
    div()
        .id(id)
        .size(z(HEADER_BUTTON, cx))
        .flex()
        .items_center()
        .justify_center()
        .rounded(z(4., cx))
        .text_color(c(p.sub))
        .hover(|style| style.bg(c(p.hover)).text_color(c(p.ink)))
        .tooltip(move |window, cx| Tooltip::new(tooltip).build(window, cx))
        .child(Icon::new(icon).size(z(15., cx)))
}

/// A list that floats `top` below where it is placed, drawn over everything and kept on screen.
/// `corner` is the list's corner that sits there.
fn floating<E: IntoElement>(
    top: f32,
    corner: Anchor,
    list: E,
    cx: &App,
) -> impl IntoElement + use<E> {
    let right = matches!(corner, Anchor::TopRight | Anchor::BottomRight);
    div()
        .absolute()
        .top(z(top, cx))
        .map(|el| if right { el.right_0() } else { el.left_0() })
        .child(
            deferred(
                anchored()
                    .anchor(corner)
                    .snap_to_window_with_margin(px(8.))
                    .child(list),
            )
            .with_priority(1),
        )
}

fn popup_box(cx: &App) -> Div {
    let p = theme::colors(cx);
    div()
        .occlude()
        .min_w(z(180., cx))
        .py(z(4., cx))
        .flex()
        .flex_col()
        .bg(c(p.paper))
        .border_1()
        .border_color(c(p.line))
        .rounded(z(6., cx))
        .text_size(z(size::UI_TEXT, cx))
        .text_color(c(p.ink))
}

/// One line of a floating list; `highlighted` is the one Enter picks.
fn popup_item(
    id: impl Into<ElementId>,
    icon: Option<IconName>,
    label: &'static str,
    highlighted: bool,
    cx: &App,
) -> Stateful<Div> {
    let p = theme::colors(cx);
    div()
        .id(id)
        .mx(z(4., cx))
        .px(z(8., cx))
        .h(z(size::TREE_ROW, cx))
        .flex()
        .items_center()
        .gap(z(8., cx))
        .rounded(z(4., cx))
        .when(highlighted, |el| {
            el.bg(c(p.sel_bg)).text_color(c(p.sel_ink))
        })
        .when(!highlighted, |el| el.hover(|style| style.bg(c(p.hover))))
        .child(
            div()
                .size(z(14., cx))
                .flex_none()
                .children(icon.map(|icon| Icon::new(icon).size(z(14., cx)).text_color(c(p.sub)))),
        )
        .child(label)
}

/// IntelliJ's New popup under the selected row: a file or a folder. Up, Down and Enter pick, as do
/// F and D; Escape closes it.
fn new_popup(
    depth: usize,
    choice: usize,
    panel: WeakEntity<ProjectPanel>,
    cx: &App,
) -> impl IntoElement {
    let p = theme::colors(cx);
    let kinds = [
        (EntryKind::File, IconName::File, "File"),
        (EntryKind::Dir, IconName::Folder, "Directory"),
    ];
    div()
        .absolute()
        .left(z(row_padding(depth) + CHEVRON_SLOT, cx))
        .top_0()
        .child(floating(
            size::TREE_ROW,
            Anchor::TopLeft,
            popup_box(cx)
                .child(
                    div()
                        .px(z(12., cx))
                        .pb(z(4., cx))
                        .text_size(z(11.5, cx))
                        .text_color(c(p.sub))
                        .child("New"),
                )
                .children(
                    kinds
                        .into_iter()
                        .enumerate()
                        .map(|(index, (kind, icon, label))| {
                            let panel = panel.clone();
                            popup_item(
                                ("tree-new-kind", index),
                                Some(icon),
                                label,
                                index == choice,
                                cx,
                            )
                            .on_click(move |_, window, cx| {
                                let _ =
                                    panel.update(cx, |this, cx| this.choose_new(kind, window, cx));
                            })
                        }),
                ),
            cx,
        ))
}

/// A folder row's menu, when the window has several: Remove from Window.
fn folder_popup(
    depth: usize,
    path: PathBuf,
    panel: WeakEntity<ProjectPanel>,
    cx: &App,
) -> impl IntoElement {
    div()
        .absolute()
        .left(z(row_padding(depth) + CHEVRON_SLOT, cx))
        .top_0()
        .child(floating(
            size::TREE_ROW,
            Anchor::TopLeft,
            popup_box(cx).child(
                popup_item(
                    "tree-folder-remove",
                    Some(IconName::FolderMinus),
                    "Remove from Window",
                    false,
                    cx,
                )
                .on_click(move |_, _, cx| {
                    let _ = panel.update(cx, |this, cx| {
                        this.menu = None;
                        cx.emit(PanelEvent::RemoveFolder(path.clone()));
                        cx.notify();
                    });
                }),
            ),
            cx,
        ))
}

/// A row's name, its speed search match in semibold (medium barely shows at 13 px).
fn name_label(name: &str, query: Option<&str>) -> StyledText {
    let text = StyledText::new(name.to_string());
    match query.and_then(|query| nav::name_match(name, query)) {
        Some(range) => text.with_highlights([(
            range,
            HighlightStyle {
                font_weight: Some(FontWeight::SEMIBOLD),
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
                // Plain ink even on the selected row, like the speed search field.
                .text_color(c(p.ink))
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
        let home = std::env::var_os("HOME").map(PathBuf::from);
        // Where the folders are: one folder's row shows the folder it sits in (IntelliJ), several
        // show their own places under the window's node.
        let places: Vec<(PathBuf, SharedString)> = if self.folders.several() {
            self.folders
                .roots()
                .iter()
                .zip(self.folders.keys())
                .map(|(root, key)| (PathBuf::from(key), tilde(root, home.as_deref()).into()))
                .collect()
        } else {
            let parent = self.folders.first().parent();
            vec![(
                PathBuf::new(),
                parent
                    .map(|parent| tilde(parent, home.as_deref()))
                    .unwrap_or_default()
                    .into(),
            )]
        };
        let selected_row = selected
            .as_ref()
            .and_then(|s| rows.iter().find(|r| &r.path == s))
            .cloned();
        // The way to the selection and to the hovered row, as list lines: the input for a new
        // item is a line of its own, and the lines below it move down one.
        let stops = selected
            .as_deref()
            .map_or_else(Vec::new, |s| tree::route(&rows, s));
        let hovered = self
            .hovered
            .as_deref()
            .map_or_else(Vec::new, |h| tree::route(&rows, h));
        let items = self.items(rows);
        let count = items.len();
        let new_at = items
            .iter()
            .position(|item| matches!(item, Item::New { .. }));
        let to_line = |row: usize| row + usize::from(new_at.is_some_and(|at| row >= at));
        let stops: Vec<usize> = stops.into_iter().map(to_line).collect();
        let hovered: Vec<usize> = hovered.into_iter().map(to_line).collect();
        let now = Instant::now();
        self.route
            .aim(selected.as_deref(), &stops, now, !cx.reduce_motion());
        if self.route.moving(now) {
            window.request_animation_frame();
        }
        // A folder on the route takes the color of the line leaving it; the selection, of the
        // line reaching it.
        let on_route: Vec<(usize, u32)> = stops
            .iter()
            .enumerate()
            .map(|(depth, &line)| {
                let leg = if depth + 1 == stops.len() {
                    depth.saturating_sub(1)
                } else {
                    depth
                };
                (line, p.route[leg % p.route.len()])
            })
            .collect();
        let route_layer = {
            let (shape, ping) = (self.route.at(now), self.route.ping(now));
            let scroll = self.scroll.clone();
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    let offset = scroll.0.borrow().base_handle.offset();
                    let look = route::Look {
                        origin: point(bounds.left(), bounds.top() + offset.y),
                        row: z(size::TREE_ROW, cx),
                        column: z(column_x(0), cx),
                        indent: z(size::TREE_INDENT, cx),
                        right: bounds.right() - z(ROW_PADDING_RIGHT, cx),
                        zoom: theme::zoom(cx),
                        lines: p.route.map(c),
                        ring: c(p.ink),
                        paper: c(p.paper),
                        // Without focus the band turns neutral and fainter, like the current line.
                        selection: c(if focused { p.tree_sel } else { p.cur }),
                        hover: c(p.hover),
                    };
                    route::paint(window, bounds, &look, &shape, ping, &stops, &hovered);
                },
            )
            .absolute()
            .size_full()
        };
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
                    this.menu = None;
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
            .on_action(cx.listener(|this, _: &Open, window, cx| {
                if let Some(Menu::New { choice, .. }) = &this.menu {
                    let kind = if *choice == 0 {
                        EntryKind::File
                    } else {
                        EntryKind::Dir
                    };
                    this.choose_new(kind, window, cx);
                    return;
                }
                // Enter in a name input reaches here after the input took it.
                if this.edit.is_none() {
                    this.search = None;
                    this.open_selected(false, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &FocusEditor, window, cx| this.escape(window, cx)))
            .on_action(cx.listener(|this, _: &LeafThrough, _, cx| {
                if this.menu.is_some() {
                    return;
                }
                if this.edit.is_some() {
                    // Space is bound in the tree; let it go on and type into the name.
                    cx.propagate();
                } else {
                    this.open_selected(true, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &NewFile, window, cx| this.new_file(window, cx)))
            .on_action(cx.listener(|this, _: &Rename, window, cx| this.rename(window, cx)))
            .on_action(cx.listener(|this, _: &Trash, window, cx| this.trash(window, cx)))
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<ResizeDrag>, _, cx| {
                    // The pointer holds the middle of the gap, half a gap past the tree's edge.
                    let zoom = theme::zoom(cx);
                    let width = (event.event.position.x - event.bounds.left()).as_f32() / zoom
                        - size::ISLAND_GAP / 2.;
                    this.width = clamp_width(width);
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
            // An island, like the editor and the terminal: paper with rounded corners on the desk.
            .rounded(z(size::ISLAND_RADIUS, cx))
            .overflow_hidden()
            .bg(c(p.paper))
            .text_size(z(size::UI_TEXT, cx))
            .child(self.render_header(cx))
            .when_some(self.search.clone(), |el, query| {
                el.child(search_field(query, cx))
            })
            .child(
                // The route and the row backgrounds are painted under the list's rows.
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(route_layer)
                    .child(
                        uniform_list("project-tree", count, {
                            let selected = selected.clone();
                            let query = self.search.clone();
                            let edit = self.edit.as_ref().map(|edit| EditView {
                                input: edit.input.clone(),
                                error: edit.error.clone(),
                            });
                            let renaming = match self.edit.as_ref().map(|edit| &edit.target) {
                                Some(EditTarget::Rename { path }) => Some(path.clone()),
                                _ => None,
                            };
                            let panel = cx.weak_entity();
                            let new_choice = match &self.menu {
                                Some(Menu::New { choice, .. }) => Some(*choice),
                                _ => None,
                            };
                            let folder_menu = match &self.menu {
                                Some(Menu::Folder { path }) => Some(path.clone()),
                                _ => None,
                            };
                            // With several folders, a folder's row has a menu (right-click).
                            let folder_rows: Vec<PathBuf> = if self.folders.several() {
                                self.folders.keys().iter().map(PathBuf::from).collect()
                            } else {
                                Vec::new()
                            };
                            move |range, _, cx| {
                                items[range.clone()]
                                    .iter()
                                    .zip(range)
                                    .map(|(item, line)| {
                                        (
                                            item,
                                            on_route
                                                .iter()
                                                .find(|(l, _)| *l == line)
                                                .map(|&(_, color)| color),
                                        )
                                    })
                                    .map(|(item, route)| match (item, &edit) {
                                        (Item::Row(row), Some(edit))
                                            if renaming.as_deref() == Some(row.path.as_path()) =>
                                        {
                                            Self::render_row(
                                                row,
                                                Some(focused),
                                                route,
                                                None,
                                                Some(edit),
                                                places
                                                    .iter()
                                                    .find(|(path, _)| *path == row.path)
                                                    .map(|(_, place)| place),
                                                cx,
                                            )
                                            .into_any_element()
                                        }
                                        (Item::Row(row), _) => {
                                            let (path, kind) = (row.path.clone(), row.kind);
                                            let is_selected =
                                                selected.as_deref() == Some(row.path.as_path());
                                            let popup =
                                                new_choice.filter(|_| is_selected).map(|choice| {
                                                    new_popup(row.depth, choice, panel.clone(), cx)
                                                        .into_any_element()
                                                });
                                            let folder_popup = folder_menu
                                                .as_ref()
                                                .filter(|path| **path == row.path)
                                                .map(|path| {
                                                    folder_popup(
                                                        row.depth,
                                                        path.clone(),
                                                        panel.clone(),
                                                        cx,
                                                    )
                                                    .into_any_element()
                                                });
                                            let is_folder = folder_rows.contains(&row.path);
                                            let panel = panel.clone();
                                            Self::render_row(
                                                row,
                                                is_selected.then_some(focused),
                                                route,
                                                query.as_deref(),
                                                None,
                                                places
                                                    .iter()
                                                    .find(|(path, _)| *path == row.path)
                                                    .map(|(_, place)| place),
                                                cx,
                                            )
                                            .children(popup)
                                            .children(folder_popup)
                                            .when(is_folder, |el| {
                                                let (panel, path) =
                                                    (panel.clone(), row.path.clone());
                                                el.on_mouse_down(
                                                    MouseButton::Right,
                                                    move |_, window, cx| {
                                                        let _ = panel.update(cx, |this, cx| {
                                                            this.open_folder_menu(
                                                                path.clone(),
                                                                window,
                                                                cx,
                                                            )
                                                        });
                                                    },
                                                )
                                            })
                                            .on_hover({
                                                let (panel, path) =
                                                    (panel.clone(), row.path.clone());
                                                move |&hovered, _, cx| {
                                                    let _ = panel.update(cx, |this, cx| {
                                                        this.hover_row(&path, hovered, cx)
                                                    });
                                                }
                                            })
                                            .on_click(move |event, _, cx| {
                                                let count = event.click_count();
                                                let _ = panel.update(cx, |this, cx| {
                                                    this.click_row(path.clone(), kind, count, cx)
                                                });
                                            })
                                            .into_any_element()
                                        }
                                        (Item::New { depth }, Some(edit)) => {
                                            Self::render_new(*depth, edit, cx).into_any_element()
                                        }
                                        (Item::New { .. }, None) => div().into_any_element(),
                                    })
                                    .collect()
                            }
                        })
                        .track_scroll(&self.scroll)
                        .size_full(),
                    ),
            )
            .when(self.show_key_hints, |el| el.child(key_hints(hints, cx)))
            .child(crate::workspace::island_corners(cx))
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
        Some(row) if row.path.as_os_str().is_empty() => &[("→", "Expand"), ("⌘⇧C", "Copy path")],
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

    use super::{
        CHEVRON_SLOT, clamp_width, column_x, hint_keys, name_selection, row_padding, rows_per_page,
        tilde,
    };

    fn folder(expanded: bool, loops: bool) -> Row {
        Row {
            path: PathBuf::from("self"),
            name: "self".into(),
            depth: 0,
            kind: EntryKind::Dir,
            expanded,
            ignored: false,
            loops,
        }
    }

    fn labels(row: Option<&Row>) -> Vec<&'static str> {
        hint_keys(row).iter().map(|&(_, label)| label).collect()
    }

    #[test]
    fn a_rename_selects_the_name_up_to_its_extension() {
        assert_eq!(name_selection("paths.md", EntryKind::File), 0..5);
        assert_eq!(name_selection("archive.tar.gz", EntryKind::File), 0..11);
        assert_eq!(name_selection(".gitignore", EntryKind::File), 0..10);
        assert_eq!(name_selection("Makefile", EntryKind::File), 0..8);
        assert_eq!(name_selection("v1.2", EntryKind::Dir), 0..4);
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
    fn the_open_folder_offers_no_parent() {
        let root = Row {
            path: PathBuf::new(),
            ..folder(false, false)
        };
        assert_eq!(labels(Some(&root)), ["Expand", "Copy path"]);
        let open = Row {
            expanded: true,
            ..root
        };
        assert_eq!(
            labels(Some(&open)),
            ["Collapse", "First child", "Copy path"]
        );
    }

    #[test]
    fn columns_follow_the_design_formula() {
        // The design puts the column of depth L at x = 16 + 14L.
        for level in 0..5 {
            assert_eq!(column_x(level), 16. + 14. * level as f32);
        }
    }

    #[test]
    fn a_folder_chevron_sits_on_its_column() {
        for depth in 0..5 {
            assert_eq!(row_padding(depth) + CHEVRON_SLOT / 2., column_x(depth));
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
