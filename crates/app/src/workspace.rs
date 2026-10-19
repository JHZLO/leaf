//! One window: the tree, the tabs, the title and status bars, and the actions that span them.
//! Also which window a folder opens in, and the session all the windows share.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use gpui_kit::{prelude::FluentBuilder as _, *};
use leaf_project::{paths, session, watch};

use crate::{
    editor_tabs::{EditorTabs, TabsChanged},
    empty_state,
    finder::{self, Finder, FinderEvent},
    keymap::{
        CloseTab, CopyPath, CopyRelativePath, FocusEditor, FocusTree, NewWindow, NextTab, Open,
        OpenFolder, PrevTab, Quit, RevealInFinder, Save, SelectNext, SelectPrev, ToggleFinder,
        TogglePreview, ToggleSearch, ToggleTerminal, WORKSPACE, ZoomIn, ZoomOut, ZoomReset,
    },
    project_panel::{FileEvent, PanelEvent, ProjectPanel, ResizeDrag},
    search_panel::{self, SearchEvent, SearchPanel},
    status_bar::{self, COPIED_FOR, Copied, FADE, Status},
    terminal_panel::{TerminalEvent, TerminalPanel},
    theme::{self, ZOOM_STEPS, c, size, z},
    title_bar,
};

/// What to do once unsaved changes may be dropped.
enum AfterDiscard {
    Quit,
    CloseWindow,
}

/// What floats over the window: the file finder or the folder search, one at a time.
enum OverlayView {
    Finder(Entity<Finder>),
    Search(Entity<SearchPanel>),
}

/// What the gap above the terminal drags; the terminal resizes as it moves.
struct TerminalResize;

/// The shell under the editor, kept while hidden so its session survives Alt+F12.
struct Terminal {
    view: Entity<TerminalPanel>,
    visible: bool,
    _subscription: Subscription,
}

struct Overlay {
    view: OverlayView,
    /// Where focus goes back when the overlay closes without opening a file.
    return_focus: Option<FocusHandle>,
    _subscription: Subscription,
}

pub struct Workspace {
    panel: Option<Entity<ProjectPanel>>,
    tabs: Entity<EditorTabs>,
    tree_visible: bool,
    terminal: Option<Terminal>,
    /// The terminal's height at 100% zoom, dragged from the gap above it.
    terminal_height: f32,
    overlay: Option<Overlay>,
    copied: Option<Copied>,
    copy_seq: u64,
    /// Dropping it stops watching the old folder.
    _watcher: Option<watch::Watcher>,
    /// Writes the session a moment after the last change.
    save_task: Task<()>,
    /// Which of the recent folders the empty window has selected.
    recent_selected: usize,
    window: AnyWindowHandle,
    /// The active file the tree last moved to, so it follows only real switches.
    followed: Option<PathBuf>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// Every open window's workspace and what it last saved, plus the recent folders, which all
/// windows share. The session file is written from here, so no window reads another while saving.
#[derive(Default)]
struct Windows {
    open: Vec<OpenWindow>,
    /// Folders opened before, newest first; the empty window offers them.
    recent: Vec<PathBuf>,
}

impl Global for Windows {}

struct OpenWindow {
    handle: AnyWindowHandle,
    workspace: WeakEntity<Workspace>,
    saved: Option<session::WindowState>,
}

/// Set by the offscreen UI checks: windows leaf opens itself then stay off screen and never take focus.
pub struct OffscreenWindows;

impl Global for OffscreenWindows {}

/// How far each new window sits from the last one.
const CASCADE: f32 = 25.;

/// Reads the shared part of the session: the zoom and the recent folders.
pub fn init(cx: &mut App) {
    let saved = session::load(&session::default_path());
    theme::set_zoom(if saved.zoom == 0 { 100 } else { saved.zoom }, cx);
    cx.set_global(Windows {
        open: Vec::new(),
        recent: saved.recent,
    });
}

/// The first windows: one for each of `paths` (folders, or files to open in their folder), or else
/// the windows open when leaf last quit, or else an empty one.
pub fn open_at_start(paths: Vec<PathBuf>, cx: &mut App) {
    let mut opened = false;
    for path in paths {
        opened |= open_path(&path, cx).is_some();
    }
    if !opened {
        for root in session::load(&session::default_path())
            .windows
            .into_iter()
            .filter_map(|w| w.root)
            .filter(|root| root.is_dir())
        {
            open_path(&root, cx);
        }
    }
    if live_windows(cx).is_empty() {
        open_window(None, None, cx);
    }
}

/// Opens a folder, or a file inside its folder, as leaf does for a path from the command line or
/// Finder: in the window that already has it, else in an empty window, else in a new one.
/// Returns the window it used.
pub fn open_path(path: &Path, cx: &mut App) -> Option<AnyWindowHandle> {
    let path = std::fs::canonicalize(path).ok()?;
    let (root, file) = if path.is_dir() {
        (path, None)
    } else {
        (path.parent()?.to_path_buf(), Some(path))
    };
    let empty = live_windows(cx)
        .into_iter()
        .find(|(_, workspace)| workspace.read(cx).panel.is_none())
        .map(|(handle, _)| handle);
    open_in(root, file, empty, cx)
}

/// Opens `root` chosen in `from` (Cmd+O, a recent folder): an empty window takes it itself, a
/// window with a folder leaves its own and opens a new one. Returns the window it used.
pub fn open_folder_from(
    root: PathBuf,
    from: AnyWindowHandle,
    cx: &mut App,
) -> Option<AnyWindowHandle> {
    let empty = live_windows(cx)
        .into_iter()
        .find(|(handle, workspace)| *handle == from && workspace.read(cx).panel.is_none())
        .map(|(handle, _)| handle);
    open_in(root, None, empty, cx)
}

/// Opens an empty window (File > New Window).
pub fn open_empty_window(cx: &mut App) -> Option<AnyWindowHandle> {
    open_window(None, None, cx)
}

/// A window already showing `root` (for a file, the window whose folder holds it) comes forward;
/// otherwise `empty` takes the folder, or a new window opens.
fn open_in(
    root: PathBuf,
    file: Option<PathBuf>,
    empty: Option<AnyWindowHandle>,
    cx: &mut App,
) -> Option<AnyWindowHandle> {
    let showing = live_windows(cx)
        .into_iter()
        .filter_map(|(handle, workspace)| {
            let open = workspace.read(cx).root(cx)?;
            let holds = match &file {
                Some(file) => file.starts_with(&open),
                None => open == root,
            };
            // The deepest folder wins when one open folder sits inside another.
            holds.then(|| (open.components().count(), handle, workspace))
        })
        .max_by_key(|(depth, ..)| *depth)
        .map(|(_, handle, workspace)| (handle, workspace, false));
    let target = showing.or_else(|| {
        let handle = empty?;
        let workspace = live_windows(cx).into_iter().find(|(h, _)| *h == handle)?.1;
        Some((handle, workspace, true))
    });
    let Some((handle, workspace, take_root)) = target else {
        return open_window(Some(root), file, cx);
    };
    let background =
        cx.has_global::<OffscreenWindows>() || std::env::var_os("LEAF_BACKGROUND").is_some();
    handle
        .update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| {
                if take_root {
                    workspace.set_root(root, window, cx);
                }
                if let Some(file) = file {
                    workspace.open_file(&file, window, cx);
                }
            });
            if !background {
                window.activate_window();
            }
        })
        .ok()?;
    Some(handle)
}

/// Opens a new window on `root` (or an empty one) with `file` open in it.
fn open_window(
    root: Option<PathBuf>,
    file: Option<PathBuf>,
    cx: &mut App,
) -> Option<AnyWindowHandle> {
    let offscreen = cx.has_global::<OffscreenWindows>();
    let first_size = size(px(1280.), px(800.));
    let bounds = if offscreen {
        Bounds::new(point(px(-10000.), px(-10000.)), first_size)
    } else {
        // Each new window steps down from the last one so none hides another exactly.
        let last = live_windows(cx).last().map(|(handle, _)| *handle);
        match last.and_then(|h| h.update(cx, |_, window, _| window.bounds()).ok()) {
            Some(last) => Bounds::new(last.origin + point(px(CASCADE), px(CASCADE)), last.size),
            None => Bounds::centered(None, first_size, cx),
        }
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("leaf".into()),
            appears_transparent: true,
            traffic_light_position: Some(title_bar::traffic_lights()),
        }),
        window_min_size: Some(size(px(640.), px(400.))),
        focus: !offscreen,
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| {
            let mut workspace = Workspace::new(window, cx);
            if let Some(root) = root {
                workspace.set_root(root, window, cx);
            }
            if let Some(file) = file {
                workspace.open_file(&file, window, cx);
            }
            workspace
        })
    });
    match opened {
        Ok((handle, _)) => Some(handle),
        Err(err) => {
            eprintln!("leaf: could not open a window: {err}");
            None
        }
    }
}

/// The windows still open, oldest first.
fn live_windows(cx: &App) -> Vec<(AnyWindowHandle, Entity<Workspace>)> {
    let open = cx.windows();
    cx.try_global::<Windows>()
        .map(|windows| {
            windows
                .open
                .iter()
                .filter(|w| open.contains(&w.handle))
                .filter_map(|w| Some((w.handle, w.workspace.upgrade()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// A window closed while others stay open drops out of the session. The app does not call this
/// for the last one, so leaf quits with that window still in the session and reopens it next time.
pub fn window_closed(id: WindowId, cx: &mut App) {
    cx.default_global::<Windows>()
        .open
        .retain(|w| w.handle.window_id() != id);
    write_session(cx);
}

fn write_session(cx: &App) {
    let Some(windows) = cx.try_global::<Windows>() else {
        return;
    };
    let saved = session::Session {
        windows: windows
            .open
            .iter()
            .filter_map(|w| w.saved.clone())
            .filter(|w| w.root.is_some())
            .collect(),
        zoom: cx.try_global::<theme::Zoom>().map_or(100, |z| z.0),
        recent: windows.recent.clone(),
    };
    if let Err(err) = session::save(&session::default_path(), &saved) {
        eprintln!("leaf: session not saved: {err}");
    }
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tabs = cx.new(|cx| EditorTabs::new(PathBuf::new(), cx));
        let subscriptions = vec![
            cx.subscribe(&tabs, |_, _, _: &TabsChanged, cx| cx.notify()),
            cx.on_app_quit(|this, cx| {
                this.save_session(cx);
                async {}
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                gpui_kit::component::Theme::sync_system_appearance(Some(window), cx);
            }),
        ];
        // The close button asks first when tabs have unsaved changes.
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |this, cx| {
                this.confirm_discard(AfterDiscard::CloseWindow, window, cx)
            })
            .unwrap_or(true)
        });
        // With no folder open, the window itself takes the keys (recent folders).
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let handle = window.window_handle();
        let workspace = cx.entity().downgrade();
        cx.default_global::<Windows>().open.push(OpenWindow {
            handle,
            workspace,
            saved: None,
        });
        Self {
            panel: None,
            tabs,
            tree_visible: true,
            terminal: None,
            terminal_height: size::TERMINAL_HEIGHT,
            overlay: None,
            copied: None,
            copy_seq: 0,
            _watcher: None,
            save_task: Task::ready(()),
            recent_selected: 0,
            window: handle,
            followed: None,
            focus_handle,
            _subscriptions: subscriptions,
        }
    }

    /// Recent folders that still exist, as the empty window lists them.
    fn recent_shown(&self, cx: &App) -> Vec<PathBuf> {
        cx.try_global::<Windows>()
            .map(|w| w.recent.as_slice())
            .unwrap_or_default()
            .iter()
            .filter(|r| r.is_dir())
            .take(empty_state::SHOWN_RECENT)
            .cloned()
            .collect()
    }

    fn open_recent(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(root) = self.recent_shown(cx).get(index).cloned() {
            let from = self.window;
            // After this update: choosing the window reads every workspace, this one included.
            cx.defer(move |cx| {
                open_folder_from(root, from, cx);
            });
        }
    }

    fn move_recent(&mut self, step: isize, cx: &mut Context<Self>) {
        let count = self.recent_shown(cx).len();
        if count > 0 {
            self.recent_selected =
                (self.recent_selected as isize + step).clamp(0, count as isize - 1) as usize;
            cx.notify();
        }
    }

    pub fn set_root(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // A finder or search still open looks at the old folder.
        self.overlay = None;
        let panel = cx.new(|cx| ProjectPanel::new(root.clone(), cx));
        self._subscriptions.push(cx.subscribe_in(
            &panel,
            window,
            |this, _, event: &PanelEvent, window, cx| match event {
                PanelEvent::Open { path, leaf_through } => {
                    let leaf_through = *leaf_through;
                    this.tabs
                        .update(cx, |tabs, cx| tabs.open(path, leaf_through, window, cx));
                    if !leaf_through {
                        this.focus_editor_area(window, cx);
                    }
                }
            },
        ));
        self._subscriptions.push(cx.subscribe_in(
            &panel,
            window,
            |this, _, event: &FileEvent, window, cx| {
                match event {
                    FileEvent::Renamed { from, to } => {
                        this.tabs.update(cx, |tabs, cx| tabs.renamed(from, to, cx))
                    }
                    FileEvent::Trashed { path } => this
                        .tabs
                        .update(cx, |tabs, cx| tabs.trashed(path, window, cx)),
                }
            },
        ));
        self.tabs = cx.new(|cx| EditorTabs::new(root.clone(), cx));
        self._subscriptions
            .push(cx.subscribe(&self.tabs, |this, _, _: &TabsChanged, cx| {
                this.follow_active_file(cx);
                this.schedule_save(cx);
                cx.notify();
            }));
        self._subscriptions
            .push(cx.observe(&panel, |this, _, cx| this.schedule_save(cx)));
        panel.read(cx).focus_handle(cx).focus(window, cx);
        self.panel = Some(panel.clone());
        session::remember(&mut cx.default_global::<Windows>().recent, root.clone());
        self.recent_selected = 0;
        if let Some(name) = root.file_name() {
            // What the Window menu and the Dock list this window as.
            window.set_window_title(&name.to_string_lossy());
        }
        self.watch(&root, window, cx);
        // The terminal is open from the start (the user asked for it on by default); focus stays in
        // the tree.
        self.show_terminal(window, cx);
        // A folder that had a window when leaf last saved comes back as it was left.
        if let Some(saved) = session::load(&session::default_path())
            .window(&root)
            .cloned()
        {
            panel.update(cx, |panel, cx| {
                panel.restore(&saved.expanded, saved.selected.clone(), cx);
                panel.width = saved.tree_width.unwrap_or(panel.width);
            });
            self.tabs.update(cx, |tabs, cx| {
                tabs.restore(&saved.tabs, saved.active_tab, window, cx);
                if saved.preview {
                    tabs.toggle_preview(cx);
                }
            });
        }
        self.schedule_save(cx);
        cx.notify();
    }

    /// Opens `file`, an absolute path inside the open folder, in a tab.
    fn open_file(&mut self, file: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.root(cx) else {
            return;
        };
        if let Ok(rel) = file.strip_prefix(&root) {
            let rel = rel.to_path_buf();
            self.tabs
                .update(cx, |tabs, cx| tabs.open(&rel, false, window, cx));
        }
    }

    /// Watches the folder; each batch of outside changes refreshes the tree and the open files.
    fn watch(&mut self, root: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let (tx, rx) = smol::channel::unbounded::<watch::Changes>();
        self._watcher = watch::Watcher::new(root, move |changes| {
            let _ = tx.send_blocking(changes);
        })
        .map_err(|err| eprintln!("leaf: not watching {}: {err}", root.display()))
        .ok();
        // Ends when the watcher (and with it the sender) is dropped for another folder.
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(changes) = rx.recv().await {
                let applied = this.update_in(cx, |this, window, cx| {
                    if let Some(panel) = this.panel.clone() {
                        panel.update(cx, |panel, cx| panel.refresh_dirs(&changes.dirs, cx));
                    }
                    this.tabs.update(cx, |tabs, cx| {
                        tabs.files_changed(&changes.files, window, cx)
                    });
                });
                if applied.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// True when nothing would be lost. Otherwise asks, and does `then` only if the user discards.
    fn confirm_discard(
        &mut self,
        then: AfterDiscard,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let unsaved = match then {
            AfterDiscard::CloseWindow => self.tabs.read(cx).tabs_unsaved(),
            // Quitting closes every window, so it counts them all.
            AfterDiscard::Quit => {
                let others: usize = live_windows(cx)
                    .iter()
                    .filter(|(handle, _)| *handle != self.window)
                    .map(|(_, workspace)| workspace.read(cx).tabs.read(cx).tabs_unsaved())
                    .sum();
                self.tabs.read(cx).tabs_unsaved() + others
            }
        };
        if unsaved == 0 {
            return true;
        }
        let detail = if unsaved == 1 {
            "1 file has unsaved changes.".to_string()
        } else {
            format!("{unsaved} files have unsaved changes.")
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            "Discard unsaved changes?",
            Some(&detail),
            &["Discard", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = this.update_in(cx, |_, window, cx| match then {
                AfterDiscard::Quit => cx.quit(),
                AfterDiscard::CloseWindow => window.remove_window(),
            });
        })
        .detach();
        false
    }

    /// What reopens next time: the folder, the tree, the tabs.
    fn snapshot(&self, cx: &App) -> session::WindowState {
        let mut saved = session::WindowState::default();
        if let Some(panel) = &self.panel {
            let panel = panel.read(cx);
            saved.root = Some(panel.root().to_path_buf());
            saved.expanded = panel.expanded_dirs();
            saved.selected = panel.selected().map(Path::to_path_buf);
            saved.tree_width = Some(panel.width);
        }
        let tabs = self.tabs.read(cx);
        (saved.tabs, saved.active_tab) = tabs.session_tabs(cx);
        saved.preview = tabs.preview_open();
        saved
    }

    /// Records this window's state and writes the session with every window in it.
    fn save_session(&self, cx: &mut App) {
        let snapshot = self.snapshot(cx);
        if let Some(entry) = cx
            .default_global::<Windows>()
            .open
            .iter_mut()
            .find(|w| w.handle == self.window)
        {
            entry.saved = Some(snapshot);
        }
        write_session(cx);
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save_task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = this.update(cx, |this, cx| this.save_session(cx));
        });
    }

    /// The tree shows the file in the active tab (IntelliJ's "always select opened file").
    fn follow_active_file(&mut self, cx: &mut Context<Self>) {
        let active = self.active_file(cx);
        if active == self.followed {
            return;
        }
        self.followed = active.clone();
        if let (Some(panel), Some(path)) = (self.panel.clone(), active) {
            panel.update(cx, |panel, cx| panel.reveal(&path, cx));
        }
    }

    /// The tree's selection, relative to the open folder.
    pub fn tree_selection(&self, cx: &App) -> Option<PathBuf> {
        self.panel
            .as_ref()?
            .read(cx)
            .selected()
            .map(Path::to_path_buf)
    }

    /// The active tab's text as the editor holds it (tests read it to check editing keys).
    pub fn active_text(&self, cx: &App) -> Option<String> {
        let tabs = self.tabs.read(cx);
        Some(tabs.active_tab()?.editor.read(cx).value().to_string())
    }

    /// The active tab's file, relative to the open folder.
    pub fn active_file(&self, cx: &App) -> Option<PathBuf> {
        self.tabs.read(cx).active_tab().map(|t| t.path.clone())
    }

    /// The open folder, if any.
    pub fn root(&self, cx: &App) -> Option<PathBuf> {
        self.panel.as_ref().map(|p| p.read(cx).root().to_path_buf())
    }

    /// The tree's width and the terminal's height at 100% zoom; for the UI checks.
    pub fn island_sizes(&self, cx: &App) -> (Option<f32>, f32) {
        let tree = self.panel.as_ref().map(|p| p.read(cx).width);
        (tree, self.terminal_height)
    }

    /// The lines the terminal shows, when it is open; for the UI checks.
    pub fn terminal_lines(&self, cx: &App) -> Option<Vec<String>> {
        let terminal = self.terminal.as_ref().filter(|t| t.visible)?;
        Some(terminal.view.read(cx).visible_lines(cx))
    }

    fn terminal_focused(&self, window: &Window, cx: &App) -> bool {
        self.terminal.as_ref().is_some_and(|t| {
            t.view
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
        })
    }

    /// Alt+F12, as in IntelliJ: shows the terminal (starting a shell when there is none) and gives
    /// it focus; pressed in the terminal, hides it and goes back to the editor.
    fn toggle_terminal(&mut self, _: &ToggleTerminal, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal_focused(window, cx) {
            if let Some(terminal) = &mut self.terminal {
                terminal.visible = false;
            }
            self.focus_editor_area(window, cx);
            cx.notify();
            return;
        }
        if let Some(terminal) = self.show_terminal(window, cx) {
            terminal.view.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    /// Shows the terminal, starting a shell in the open folder when there is none. None when no
    /// folder is open or the shell could not start.
    fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<&Terminal> {
        if self.terminal.is_none() {
            let root = self.root(cx)?;
            let started = match TerminalPanel::start(&root) {
                Ok(started) => started,
                Err(err) => {
                    eprintln!("leaf: could not start the shell: {err}");
                    return None;
                }
            };
            let view = cx.new(|cx| TerminalPanel::new(started, window, cx));
            let subscription = cx.subscribe_in(
                &view,
                window,
                |this, _, event: &TerminalEvent, window, cx| match event {
                    TerminalEvent::Exited => {
                        let had_focus = this.terminal_focused(window, cx);
                        this.terminal = None;
                        if had_focus {
                            this.focus_editor_area(window, cx);
                        }
                        cx.notify();
                    }
                },
            );
            self.terminal = Some(Terminal {
                view,
                visible: false,
                _subscription: subscription,
            });
        }
        let terminal = self.terminal.as_mut()?;
        terminal.visible = true;
        Some(terminal)
    }

    fn tree_focused(&self, window: &Window, cx: &App) -> bool {
        self.panel
            .as_ref()
            .is_some_and(|p| p.read(cx).focus_handle(cx).contains_focused(window, cx))
    }

    /// Nothing open here is replaced, so there is nothing to ask before opening.
    fn open_folder(&mut self, _: &OpenFolder, _: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        let from = self.window;
        cx.spawn(async move |_, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(root) = paths.into_iter().next() else {
                return;
            };
            let Ok(root) = std::fs::canonicalize(root) else {
                return;
            };
            cx.update(|cx| open_folder_from(root, from, cx));
        })
        .detach();
    }

    fn focus_tree(&mut self, _: &FocusTree, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.clone() else {
            return;
        };
        if self.tree_focused(window, cx) {
            self.tree_visible = false;
            self.focus_editor_area(window, cx);
        } else {
            self.tree_visible = true;
            panel.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    /// Opens the finder over the window, or closes it and gives focus back.
    fn toggle_finder(&mut self, _: &ToggleFinder, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            self.overlay,
            Some(Overlay {
                view: OverlayView::Finder(_),
                ..
            })
        ) {
            self.close_overlay(true, window, cx);
            return;
        }
        let Some(root) = self.root(cx) else {
            return;
        };
        let view = cx.new(|cx| Finder::new(root, window, cx));
        let subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, event: &FinderEvent, window, cx| match event {
                FinderEvent::Open(path) => {
                    this.close_overlay(false, window, cx);
                    this.tabs
                        .update(cx, |tabs, cx| tabs.open(path, false, window, cx));
                    this.focus_editor_area(window, cx);
                }
                FinderEvent::Dismiss => this.close_overlay(true, window, cx),
                FinderEvent::FocusLost => this.close_overlay(false, window, cx),
            },
        );
        let focus = view.read(cx).focus_handle(cx);
        self.show_overlay(OverlayView::Finder(view), focus, subscription, window, cx);
    }

    /// Opens the folder search over the window, or closes it and gives focus back.
    fn toggle_search(&mut self, _: &ToggleSearch, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            self.overlay,
            Some(Overlay {
                view: OverlayView::Search(_),
                ..
            })
        ) {
            self.close_overlay(true, window, cx);
            return;
        }
        let Some(root) = self.root(cx) else {
            return;
        };
        let view = cx.new(|cx| SearchPanel::new(root, window, cx));
        let subscription = cx.subscribe_in(
            &view,
            window,
            |this, _, event: &SearchEvent, window, cx| match event {
                SearchEvent::Open { path, line } => {
                    this.close_overlay(false, window, cx);
                    let line = *line;
                    this.tabs
                        .update(cx, |tabs, cx| tabs.open_at(path, line, window, cx));
                    this.focus_editor_area(window, cx);
                }
                SearchEvent::Dismiss => this.close_overlay(true, window, cx),
                SearchEvent::FocusLost => this.close_overlay(false, window, cx),
            },
        );
        let focus = view.read(cx).focus_handle(cx);
        self.show_overlay(OverlayView::Search(view), focus, subscription, window, cx);
    }

    fn show_overlay(
        &mut self,
        view: OverlayView,
        focus: FocusHandle,
        subscription: Subscription,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Switching straight from one overlay to the other keeps the original place to return to.
        let return_focus = match self.overlay.take() {
            Some(previous) => previous.return_focus,
            None => window.focused(cx),
        };
        focus.focus(window, cx);
        self.overlay = Some(Overlay {
            view,
            return_focus,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// `restore_focus` gives focus back to where it was before the overlay opened.
    fn close_overlay(&mut self, restore_focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(overlay) = self.overlay.take() else {
            return;
        };
        if restore_focus {
            match overlay.return_focus {
                Some(handle) => handle.focus(window, cx),
                None => self.focus_editor_area(window, cx),
            }
        }
        cx.notify();
    }

    fn focus_editor(&mut self, _: &FocusEditor, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_editor_area(window, cx);
    }

    fn focus_editor_area(&self, window: &mut Window, cx: &mut App) {
        let handle = self.tabs.read(cx).focus_handle(cx);
        handle.focus(window, cx);
    }

    /// What the copy actions copy: the tree selection when the tree has focus, else the active file.
    fn copy_source(&self, window: &Window, cx: &App) -> Option<(PathBuf, Option<(u32, u32)>)> {
        if self.tree_focused(window, cx) {
            let panel = self.panel.as_ref()?.read(cx);
            return panel.selected().map(|p| (p.to_path_buf(), None));
        }
        let tabs = self.tabs.read(cx);
        let tab = tabs.active_tab()?;
        Some((tab.path.clone(), tabs.active_lines(cx)))
    }

    fn copy(&mut self, text: String, label: &'static str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        self.copy_seq += 1;
        self.copied = Some(Copied {
            text,
            label,
            at: Instant::now(),
            seq: self.copy_seq,
        });
        // Redraw when the hold ends (the fade starts) and when the fade ends.
        for wait in [COPIED_FOR, COPIED_FOR + FADE] {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        cx.notify();
    }

    fn copy_path(&mut self, _: &CopyPath, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(root), Some((rel, _))) = (self.root(cx), self.copy_source(window, cx)) else {
            return;
        };
        self.copy(paths::absolute(&root, &rel), "Copied path", cx);
    }

    fn copy_relative_path(
        &mut self,
        _: &CopyRelativePath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((rel, lines)) = self.copy_source(window, cx) else {
            return;
        };
        match lines {
            Some((start, end)) => {
                let label = if start == end {
                    "Copied relative path and line"
                } else {
                    "Copied relative path and lines"
                };
                self.copy(paths::with_lines(&rel, start, end), label, cx);
            }
            None => self.copy(paths::relative(&rel), "Copied relative path", cx),
        }
    }

    /// Shows the tree selection, or the active file, in a Finder window.
    fn reveal_in_finder(
        &mut self,
        _: &RevealInFinder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(root), Some((rel, _))) = (self.root(cx), self.copy_source(window, cx)) else {
            return;
        };
        if let Err(err) = std::process::Command::new("open")
            .arg("-R")
            .arg(root.join(rel))
            .spawn()
        {
            eprintln!("leaf: could not reveal in Finder: {err}");
        }
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs
            .update(cx, |tabs, cx| tabs.save_active(window, cx));
    }

    fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs
            .update(cx, |tabs, cx| tabs.close_active(window, cx));
    }

    fn zoom_by(&mut self, step: i32, cx: &mut Context<Self>) {
        let current = cx.try_global::<theme::Zoom>().map_or(100, |z| z.0);
        let index = ZOOM_STEPS.iter().position(|&s| s == current).unwrap_or(2) as i32;
        let next = if step == 0 {
            100
        } else {
            ZOOM_STEPS[(index + step).clamp(0, ZOOM_STEPS.len() as i32 - 1) as usize]
        };
        theme::set_zoom(next, cx);
        self.schedule_save(cx);
        cx.notify();
    }
}

/// A rounded sheet of paper on the desk: the tree, the editor and the terminal are each one.
/// Its last child must be [`island_corners`].
fn island(cx: &App) -> Div {
    let p = theme::colors(cx);
    div()
        .relative()
        .rounded(z(size::ISLAND_RADIUS, cx))
        .overflow_hidden()
        .bg(c(p.paper))
}

/// Paints the desk over an island's corners, outside the rounding. GPUI clips what an island holds
/// to a rectangle, so a selected row, the editor or the terminal would otherwise square the corners.
pub(crate) fn island_corners(cx: &App) -> impl IntoElement {
    let desk = c(theme::colors(cx).desk);
    let radius = z(size::ISLAND_RADIUS, cx);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            // A quarter circle as one cubic curve: its control points sit this far from the corner.
            let pull = radius * (1. - 0.5523);
            let (left, right, top, bottom) =
                (bounds.left(), bounds.right(), bounds.top(), bounds.bottom());
            for (x, y, dx, dy) in [
                (left, top, 1., 1.),
                (right, top, -1., 1.),
                (left, bottom, 1., -1.),
                (right, bottom, -1., -1.),
            ] {
                let mut corner = PathBuilder::fill();
                corner.move_to(point(x, y));
                corner.line_to(point(x + radius * dx, y));
                corner.cubic_bezier_to(
                    point(x, y + radius * dy),
                    point(x + pull * dx, y),
                    point(x, y + pull * dy),
                );
                corner.close();
                if let Ok(path) = corner.build() {
                    window.paint_path(path, desk);
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let tabs = self.tabs.read(cx);
        let active = tabs.active_tab();
        let folder = self
            .root(cx)
            .and_then(|r| r.file_name().map(|n| n.to_string_lossy().to_string()));
        let file = active.map(|t| paths::relative(&t.path));
        let stem = if self.tree_focused(window, cx) {
            self.panel
                .as_ref()
                .and_then(|p| p.read(cx).selected().map(paths::relative))
        } else {
            active.map(|t| match tabs.active_lines(cx) {
                Some((start, end)) => paths::with_lines(&t.path, start, end),
                None => paths::relative(&t.path),
            })
        };
        let stem = match self.panel {
            // A settings file that could not be used says so until a file is open.
            Some(_) => tabs
                .error
                .clone()
                .or_else(|| {
                    active
                        .is_none()
                        .then(|| cx.global::<crate::UserSettings>().error.clone())
                        .flatten()
                })
                .or(stem),
            None => Some("No folder open".to_string()),
        };
        let language = active.map(|t| status_bar::language_label(&t.language));
        let line_ending = active.map(|t| match t.line_ending {
            leaf_project::fs::LineEnding::Lf => "LF",
            leaf_project::fs::LineEnding::Crlf => "CRLF",
        });
        let zoom = cx.try_global::<theme::Zoom>().map_or(100, |z| z.0);
        let copied = self.copied.clone();

        div()
            .key_context(if self.panel.is_some() {
                WORKSPACE
            } else {
                "Workspace empty"
            })
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(|_, _: &NewWindow, _, cx| {
                cx.defer(|cx| {
                    open_empty_window(cx);
                })
            }))
            .on_action(cx.listener(Self::focus_tree))
            .on_action(cx.listener(Self::focus_editor))
            .on_action(cx.listener(Self::toggle_finder))
            .on_action(cx.listener(Self::toggle_search))
            .on_action(cx.listener(Self::toggle_terminal))
            .on_action(cx.listener(Self::copy_path))
            .on_action(cx.listener(Self::copy_relative_path))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::reveal_in_finder))
            .when(self.panel.is_none(), |el| {
                el.on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.move_recent(-1, cx)))
                    .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.move_recent(1, cx)))
                    .on_action(cx.listener(|this, _: &Open, _, cx| {
                        this.open_recent(this.recent_selected, cx)
                    }))
            })
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(|this, _: &Quit, window, cx| {
                // Without unsaved changes the app-wide handler quits.
                if !this.confirm_discard(AfterDiscard::Quit, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| {
                this.tabs
                    .update(cx, |tabs, cx| tabs.activate_offset(-1, window, cx))
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.tabs
                    .update(cx, |tabs, cx| tabs.activate_offset(1, window, cx))
            }))
            .on_action(cx.listener(|this, _: &TogglePreview, _, cx| {
                this.tabs.update(cx, |tabs, cx| tabs.toggle_preview(cx));
                this.schedule_save(cx);
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom_by(1, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom_by(-1, cx)))
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| this.zoom_by(0, cx)))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(c(p.desk))
            .text_color(c(p.ink))
            .child(title_bar::render(folder.as_deref(), file.as_deref(), cx))
            .child(
                // The islands on the desk (IntelliJ's layout): the tree and the editor side by side,
                // the terminal across the bottom. The gaps between them resize them.
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .px(z(size::ISLAND_GAP, cx))
                    .pb(z(size::ISLAND_GAP, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .when_some(
                                self.panel.clone().filter(|_| self.tree_visible),
                                |el, panel| {
                                    el.child(panel).child(
                                        div()
                                            .id("tree-resize")
                                            .flex_none()
                                            .w(z(size::ISLAND_GAP, cx))
                                            .h_full()
                                            .cursor_col_resize()
                                            .on_drag(ResizeDrag, |_, _, _, cx| {
                                                cx.new(|_| EmptyView)
                                            }),
                                    )
                                },
                            )
                            .child(
                                island(cx)
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .map(|el| match self.panel {
                                        Some(_) => el.child(self.tabs.clone()),
                                        None => {
                                            let this = cx.entity().downgrade();
                                            el.child(empty_state::render(
                                                &self.recent_shown(cx),
                                                self.recent_selected,
                                                move |index, _, cx| {
                                                    let _ = this.update(cx, |this, cx| {
                                                        this.open_recent(index, cx)
                                                    });
                                                },
                                                cx,
                                            ))
                                        }
                                    })
                                    .child(island_corners(cx)),
                            ),
                    )
                    .when_some(
                        self.terminal.as_ref().filter(|t| t.visible),
                        |el, terminal| {
                            el.child(
                                div()
                                    .id("terminal-resize")
                                    .flex_none()
                                    .h(z(size::ISLAND_GAP, cx))
                                    .w_full()
                                    .cursor_row_resize()
                                    .on_drag(TerminalResize, |_, _, _, cx| cx.new(|_| EmptyView)),
                            )
                            .child(
                                island(cx)
                                    .flex_none()
                                    .h(z(self.terminal_height, cx))
                                    .on_drag_move(cx.listener(
                                        |this,
                                         event: &DragMoveEvent<TerminalResize>,
                                         window,
                                         cx| {
                                            let zoom = theme::zoom(cx);
                                            // The pointer holds the middle of the gap above.
                                            let height = (event.bounds.bottom()
                                                - event.event.position.y)
                                                .as_f32()
                                                / zoom
                                                - size::ISLAND_GAP / 2.;
                                            // Leave the editor room; the window may be short.
                                            let most = (window.viewport_size().height.as_f32()
                                                / zoom
                                                - 200.)
                                                .max(size::TERMINAL_MIN);
                                            this.terminal_height =
                                                height.clamp(size::TERMINAL_MIN, most);
                                            cx.notify();
                                        },
                                    ))
                                    .child(terminal.view.clone())
                                    .child(island_corners(cx)),
                            )
                        },
                    ),
            )
            .child(status_bar::render(
                Status {
                    stem: stem.as_deref(),
                    copied: copied.as_ref(),
                    language: language.as_deref(),
                    line_ending,
                    zoom,
                },
                cx,
            ))
            .when_some(self.overlay.as_ref(), |el, overlay| match &overlay.view {
                OverlayView::Finder(view) => el.child(finder::overlay(view.clone(), cx)),
                OverlayView::Search(view) => el.child(search_panel::overlay(view.clone(), cx)),
            })
    }
}
