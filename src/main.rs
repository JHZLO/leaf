//! Phase 0 spike: measures gpui-kit against the design budgets.
//! Throwaway code that lives only on the `spike/gpui-kit` branch.
//!
//! Usage:
//!   leaf-spike editor <file>   editor (+ Markdown preview with cmd-shift-v)
//!   leaf-spike tree <rows>     a uniform_list tree with that many rows
//! `LEAF_SPIKE_EXIT_MS=<ms>` quits after that long, for scripted measurement.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

use gpui_kit::component::{
    ActiveTheme as _, h_flex,
    input::{Editor, EditorState, InputEvent, RopeExt as _},
    text::{TextView, TextViewState},
    v_flex,
};
use gpui_kit::{prelude::FluentBuilder as _, *};

static START: OnceLock<Instant> = OnceLock::new();

fn since_start_ms() -> f64 {
    START
        .get()
        .map_or(0.0, |t| t.elapsed().as_secs_f64() * 1000.0)
}

fn log(event: &str, value: impl std::fmt::Display) {
    eprintln!("leaf-spike {event}={value}");
}

actions!(leaf_spike, [CopyPath, CopyRelativePath, TogglePreview]);

struct EditorSpike {
    path: PathBuf,
    editor: Entity<EditorState>,
    preview: Option<Entity<TextViewState>>,
    preview_task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EditorSpike {
    fn new(path: PathBuf, text: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let language = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("text")
            .to_string();
        let started = Instant::now();
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(language.clone())
                .line_number(true)
                .searchable(true)
                .default_value(text)
        });
        log(
            "editor_state_ms",
            format!("{:.1}", started.elapsed().as_secs_f64() * 1000.0),
        );
        log("language", &language);

        window.on_next_frame(|_, _| log("first_frame_ms", format!("{:.1}", since_start_ms())));

        let focus = editor.focus_handle(cx);
        window.defer(cx, move |window, cx| focus.focus(window, cx));

        let subscriptions = vec![cx.subscribe(&editor, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.schedule_preview(cx);
            }
        })];

        // Scripted runs open the preview at once, then re-render it a second later as if after typing.
        if std::env::var_os("LEAF_SPIKE_PREVIEW").is_some() {
            cx.defer_in(window, |this, window, cx| {
                this.toggle_preview(&TogglePreview, window, cx)
            });
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(1000))
                    .await;
                let _ = this.update(cx, |this, cx| this.refresh_preview(cx));
            })
            .detach();
        }

        Self {
            path,
            editor,
            preview: None,
            preview_task: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    // Re-render the preview only after typing pauses for 150ms (PRD 5.6).
    fn schedule_preview(&mut self, cx: &mut Context<Self>) {
        if self.preview.is_none() {
            return;
        }
        self.preview_task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;
            let _ = this.update(cx, |this, cx| this.refresh_preview(cx));
        });
    }

    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        let Some(preview) = self.preview.clone() else {
            return;
        };
        let source = self.editor.read(cx).value().to_string();
        let started = Instant::now();
        preview.update(cx, |state, cx| state.set_text(&source, cx));
        log(
            "preview_set_text_ms",
            format!("{:.1}", started.elapsed().as_secs_f64() * 1000.0),
        );
    }

    fn toggle_preview(&mut self, _: &TogglePreview, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview.take().is_none() {
            let source = self.editor.read(cx).value().to_string();
            let started = Instant::now();
            self.preview = Some(cx.new(|cx| TextViewState::markdown(&source, cx)));
            let built = started.elapsed().as_secs_f64() * 1000.0;
            log("preview_build_ms", format!("{built:.1}"));
            window.on_next_frame(move |_, _| {
                log(
                    "preview_frame_ms",
                    format!("{:.1}", started.elapsed().as_secs_f64() * 1000.0),
                )
            });
        }
        cx.notify();
    }

    fn copy(&self, text: String, cx: &mut App) {
        log("copied", &text);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn copy_path(&mut self, _: &CopyPath, _: &mut Window, cx: &mut Context<Self>) {
        let absolute = std::fs::canonicalize(&self.path).unwrap_or(self.path.clone());
        self.copy(absolute.display().to_string(), cx);
    }

    // `path:line`, or `path:start-end` when the selection spans lines (PRD 5.3).
    fn copy_relative_path(&mut self, _: &CopyRelativePath, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.editor.read(cx);
        let range = state.selected_range();
        let start = state.text().offset_to_position(range.start).line + 1;
        let end = state.text().offset_to_position(range.end).line + 1;
        let lines = if start == end {
            format!("{start}")
        } else {
            format!("{start}-{end}")
        };
        let text = format!("{}:{lines}", self.path.display());
        self.copy(text, cx);
    }
}

impl Render for EditorSpike {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let position = self.editor.read(cx).cursor_position();
        h_flex()
            .key_context("LeafSpike")
            .on_action(cx.listener(Self::copy_path))
            .on_action(cx.listener(Self::copy_relative_path))
            .on_action(cx.listener(Self::toggle_preview))
            .size_full()
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .child(Editor::new(&self.editor).bordered(false).flex_1())
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{}:{}", self.path.display(), position.line + 1)),
                    ),
            )
            .children(self.preview.as_ref().map(|preview| {
                div()
                    .id("preview")
                    .flex_1()
                    .h_full()
                    .overflow_y_scroll()
                    .p_6()
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .child(TextView::new(preview))
            }))
    }
}

struct TreeSpike {
    rows: usize,
    selected: usize,
}

impl Render for TreeSpike {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected;
        let selected_bg = cx.theme().list_active;
        div().size_full().child(
            uniform_list("tree", self.rows, move |range, _, _| {
                range
                    .map(|i| {
                        let depth = i % 4;
                        div()
                            .h(px(22.))
                            .pl(px(10. + depth as f32 * 14.))
                            .when(i == selected, |row| row.bg(selected_bg))
                            .child(format!("entry-{i:05}.md"))
                    })
                    .collect()
            })
            .size_full(),
        )
    }
}

fn main() {
    START.get_or_init(Instant::now);
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "editor".into());
    let arg = args.next();

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        log("init_ms", format!("{:.1}", since_start_ms()));
        cx.bind_keys([
            KeyBinding::new("cmd-shift-c", CopyPath, Some("LeafSpike")),
            KeyBinding::new("cmd-alt-shift-c", CopyRelativePath, Some("LeafSpike")),
            KeyBinding::new("cmd-shift-v", TogglePreview, Some("LeafSpike")),
        ]);

        if let Some(ms) = std::env::var("LEAF_SPIKE_EXIT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
        {
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(ms))
                    .await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        }

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };

        match mode.as_str() {
            "tree" => {
                let rows = arg.and_then(|a| a.parse().ok()).unwrap_or(10_000);
                gpui_kit::open_window(options, cx, move |window, cx| {
                    window.on_next_frame(|_, _| {
                        log("first_frame_ms", format!("{:.1}", since_start_ms()))
                    });
                    cx.new(|_| TreeSpike { rows, selected: 3 })
                })
                .expect("open window");
            }
            _ => {
                let path = PathBuf::from(arg.expect("usage: leaf-spike editor <file>"));
                let started = Instant::now();
                let text = std::fs::read_to_string(&path).expect("read file");
                log(
                    "read_ms",
                    format!("{:.1}", started.elapsed().as_secs_f64() * 1000.0),
                );
                log("bytes", text.len());
                log("lines", text.lines().count());
                gpui_kit::open_window(options, cx, move |window, cx| {
                    cx.new(|cx| EditorSpike::new(relative_to_cwd(&path), text, window, cx))
                })
                .expect("open window");
            }
        }
        // Scripted runs stay in the background so they do not steal focus while measuring.
        if std::env::var_os("LEAF_SPIKE_EXIT_MS").is_none() {
            cx.activate(true);
        }
    });
}

fn relative_to_cwd(path: &Path) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_default();
    path.strip_prefix(&cwd)
        .map(Path::to_path_buf)
        .unwrap_or(path.to_path_buf())
}
