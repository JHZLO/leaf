//! The status bar. Its left end is the stem: the path you would copy, and the place copying is confirmed.

use std::time::{Duration, Instant};

use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::theme::{self, c, size, z};

/// How long the copied state stays before it fades back.
pub const COPIED_FOR: Duration = Duration::from_millis(1500);

/// What was just copied, shown on the stem until [`COPIED_FOR`] has passed.
#[derive(Clone, Debug)]
pub struct Copied {
    pub text: String,
    /// The format, as shown after the stem (`Copied relative path and line`).
    pub label: &'static str,
    pub at: Instant,
}

pub struct Status<'a> {
    /// The stem: the relative path (and cursor line) of what has focus.
    pub stem: Option<&'a str>,
    pub copied: Option<&'a Copied>,
    pub language: Option<&'a str>,
    pub line_ending: Option<&'a str>,
    pub zoom: u32,
}

pub fn render(status: Status<'_>, cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    let copied = status
        .copied
        .filter(|copied| copied.at.elapsed() < COPIED_FOR);
    let stem_text = copied
        .map(|copied| copied.text.clone())
        .or_else(|| status.stem.map(str::to_string))
        .unwrap_or_default();
    let stem = div()
        .px_2()
        .rounded(px(4.))
        .font_family("Menlo")
        .text_size(z(11.5, cx))
        .text_color(c(if copied.is_some() { p.sel_ink } else { p.ink }))
        .when(copied.is_some(), |el| el.bg(c(p.sel_bg)))
        .child(stem_text);
    div()
        .h(z(size::STATUS_BAR, cx))
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .pl_2()
        .pr_3()
        .bg(c(p.side))
        .border_t_1()
        .border_color(c(p.line))
        .text_size(z(12., cx))
        .text_color(c(p.sub))
        .child(
            div().flex().items_center().gap_2p5().child(stem).children(
                copied.map(|copied| div().text_color(c(p.accent_text)).child(copied.label)),
            ),
        )
        .child(
            div()
                .flex()
                .gap_4()
                .children(status.language.map(str::to_string))
                .children(status.line_ending.map(str::to_string))
                .children(status.language.map(|_| "UTF-8"))
                .child(format!("{}%", status.zoom)),
        )
}
