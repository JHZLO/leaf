//! The Markdown preview beside the editor (Cmd+Shift+V).

use std::time::Duration;

use gpui_kit::base::{TextView, TextViewStyle};
use gpui_kit::component::{ActiveTheme as _, input::EditorState, text::TextViewState};
use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::theme::{self, Palette, c, z};

/// Re-render only after typing pauses this long, so a long document does not re-render per key.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// Body text size at 100% zoom; headings below h2 keep gpui-kit's own ratios to it.
const BODY: f32 = 14.;

pub struct Preview {
    state: Entity<TextViewState>,
    /// The Markdown editor shown; none when the active tab is not Markdown.
    source: Option<Entity<EditorState>>,
    task: Task<()>,
}

impl Preview {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            state: cx.new(|cx| TextViewState::markdown("", cx)),
            source: None,
            task: Task::ready(()),
        }
    }

    /// Shows `source` now, dropping a pending update.
    pub fn show(&mut self, source: Option<Entity<EditorState>>, cx: &mut Context<Self>) {
        self.task = Task::ready(());
        self.source = source;
        self.update_text(cx);
    }

    /// Shows the source's new text once typing pauses.
    pub fn schedule(&mut self, cx: &mut Context<Self>) {
        self.task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.update_text(cx));
        });
    }

    fn update_text(&mut self, cx: &mut Context<Self>) {
        if let Some(source) = &self.source {
            // Read here, after the pause, so typing does not copy the document on every key.
            let text = source.read(cx).value();
            self.state.update(cx, |state, cx| state.set_text(&text, cx));
        }
        cx.notify();
    }
}

/// The design's preview typography, as far as the gpui-kit text view takes it.
fn preview_style(p: &Palette, cx: &App) -> TextViewStyle {
    let zoom = theme::zoom(cx);
    let title = c(p.syn_title);
    let none = transparent_black();
    TextViewStyle::default()
        .with_foreground(c(p.ink))
        // Blockquotes.
        .with_muted_foreground(c(p.sub))
        .with_link(c(p.syn_type))
        .with_selection(c(p.sel_bg))
        .with_code_background(c(p.side))
        .with_border(c(p.line))
        .with_heading(move |level| {
            let size = match level {
                1 => 24.,
                2 => 18.,
                3 => BODY * 17. / 15.,
                4 => BODY,
                5 => BODY * 14. / 15.,
                _ => BODY * 13. / 15.,
            };
            StyleRefinement::default()
                .text_size(px(size * zoom))
                .line_height(relative(1.3))
                .text_color(title)
        })
        .with_inline_code(HighlightStyle {
            color: Some(c(p.syn_literal)),
            background_color: Some(c(p.side)),
            ..Default::default()
        })
        .with_code_block(StyleRefinement::default().rounded(z(4., cx)))
        // Tables are 1 px rules under each row and nothing else: no frame, no column lines, no fills.
        .with_table(
            StyleRefinement::default()
                .text_size(z(13., cx))
                .bg(none)
                .border_0()
                .border_b_1(),
        )
        .with_table_head(
            StyleRefinement::default()
                .bg(none)
                .font_weight(FontWeight::SEMIBOLD),
        )
        .with_table_cell(
            StyleRefinement::default()
                .border_color(none)
                .pl(px(0.))
                .pr(z(10., cx))
                .py(z(6., cx)),
        )
        .with_dark(cx.theme().is_dark())
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        div()
            .id("preview")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .bg(c(p.paper))
            .border_l_1()
            .border_color(c(p.line))
            .px(z(40., cx))
            .py(z(26., cx))
            .text_color(c(p.ink))
            .map(|el| {
                if self.source.is_some() {
                    el.text_size(z(BODY, cx))
                        .line_height(relative(1.7))
                        .child(TextView::new(&self.state).style(preview_style(p, cx)))
                } else {
                    el.text_size(z(13., cx))
                        .text_color(c(p.sub))
                        .child("Preview shows Markdown files")
                }
            })
    }
}
