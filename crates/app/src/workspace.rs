//! One window: the tree, the tabs, the title and status bars, and the actions that span them.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use gpui_kit::{prelude::FluentBuilder as _, *};
use leaf_project::{paths, session};

use crate::{
    editor_tabs::{EditorTabs, TabsChanged},
    empty_state,
    keymap::{
        CloseTab, CopyPath, CopyRelativePath, FocusEditor, FocusTree, NextTab, OpenFolder, PrevTab,
        Save, TogglePreview, WORKSPACE, ZoomIn, ZoomOut, ZoomReset,
    },
    project_panel::{PanelEvent, ProjectPanel},
    status_bar::{self, COPIED_FOR, Copied, FADE, Status},
    theme::{self, ZOOM_STEPS, c},
    title_bar,
};

pub struct Workspace {
    panel: Option<Entity<ProjectPanel>>,
    tabs: Entity<EditorTabs>,
    tree_visible: bool,
    copied: Option<Copied>,
    copy_seq: u64,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// Opens the window. `arg` is a folder to open, or a file to open inside its folder.
pub fn open(arg: Option<PathBuf>, cx: &mut App) {
    let arg = arg.and_then(|p| std::fs::canonicalize(p).ok());
    let (root, file) = match arg {
        Some(path) if path.is_dir() => (Some(path), None),
        Some(path) => (path.parent().map(Path::to_path_buf), Some(path)),
        None => (
            session::load(&session::default_path())
                .root
                .filter(|r| r.is_dir()),
            None,
        ),
    };
    let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("leaf".into()),
            appears_transparent: true,
            traffic_light_position: Some(title_bar::traffic_lights()),
        }),
        window_min_size: Some(size(px(640.), px(400.))),
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| {
            let mut workspace = Workspace::new(window, cx);
            if let Some(root) = root {
                workspace.set_root(root, window, cx);
            }
            if let (Some(file), Some(panel)) = (file, workspace.panel.clone()) {
                let root = panel.read(cx).root().to_path_buf();
                if let Ok(rel) = file.strip_prefix(&root) {
                    let rel = rel.to_path_buf();
                    workspace
                        .tabs
                        .update(cx, |tabs, cx| tabs.open(&rel, false, window, cx));
                }
            }
            workspace
        })
    });
    if let Err(err) = opened {
        eprintln!("leaf: could not open a window: {err}");
    }
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tabs = cx.new(|cx| EditorTabs::new(PathBuf::new(), cx));
        let subscriptions = vec![
            cx.subscribe(&tabs, |_, _, _: &TabsChanged, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, window, cx| {
                gpui_kit::component::Theme::sync_system_appearance(Some(window), cx);
            }),
        ];
        Self {
            panel: None,
            tabs,
            tree_visible: true,
            copied: None,
            copy_seq: 0,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    pub fn set_root(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
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
        self.tabs = cx.new(|cx| EditorTabs::new(root.clone(), cx));
        self._subscriptions
            .push(cx.subscribe(&self.tabs, |_, _, _: &TabsChanged, cx| cx.notify()));
        panel.read(cx).focus_handle(cx).focus(window, cx);
        self.panel = Some(panel);
        let mut saved = session::load(&session::default_path());
        saved.root = Some(root);
        let _ = session::save(&session::default_path(), &saved);
        cx.notify();
    }

    fn root(&self, cx: &App) -> Option<PathBuf> {
        self.panel.as_ref().map(|p| p.read(cx).root().to_path_buf())
    }

    fn tree_focused(&self, window: &Window, cx: &App) -> bool {
        self.panel
            .as_ref()
            .is_some_and(|p| p.read(cx).focus_handle(cx).contains_focused(window, cx))
    }

    fn open_folder(&mut self, _: &OpenFolder, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(root) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| this.set_root(root, window, cx));
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
        cx.notify();
    }
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
            Some(_) => tabs.error.clone().or(stem),
            None => Some("No folder open".to_string()),
        };
        let language = active.map(|t| t.language.to_string());
        let line_ending = active.map(|t| match t.line_ending {
            leaf_project::fs::LineEnding::Lf => "LF",
            leaf_project::fs::LineEnding::Crlf => "CRLF",
        });
        let zoom = cx.try_global::<theme::Zoom>().map_or(100, |z| z.0);
        let copied = self.copied.clone();

        div()
            .key_context(WORKSPACE)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open_folder))
            .on_action(cx.listener(Self::focus_tree))
            .on_action(cx.listener(Self::focus_editor))
            .on_action(cx.listener(Self::copy_path))
            .on_action(cx.listener(Self::copy_relative_path))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::close_tab))
            .on_action(cx.listener(|this, _: &PrevTab, window, cx| {
                this.tabs
                    .update(cx, |tabs, cx| tabs.activate_offset(-1, window, cx))
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| {
                this.tabs
                    .update(cx, |tabs, cx| tabs.activate_offset(1, window, cx))
            }))
            .on_action(cx.listener(|this, _: &TogglePreview, _, cx| {
                this.tabs.update(cx, |tabs, cx| tabs.toggle_preview(cx))
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom_by(1, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom_by(-1, cx)))
            .on_action(cx.listener(|this, _: &ZoomReset, _, cx| this.zoom_by(0, cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(c(p.paper))
            .text_color(c(p.ink))
            .child(title_bar::render(folder.as_deref(), file.as_deref(), cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when_some(
                        self.panel.clone().filter(|_| self.tree_visible),
                        |el, panel| el.child(panel),
                    )
                    .map(|el| match self.panel {
                        Some(_) => el.child(self.tabs.clone()),
                        None => el.child(empty_state::render(cx)),
                    }),
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
    }
}
