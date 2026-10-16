//! The Markdown preview beside the editor (Cmd+Shift+V).

use std::{cell::Cell, ops::Range, rc::Rc, time::Duration};

use gpui_kit::base::{TextView, TextViewStyle};
use gpui_kit::component::{
    ActiveTheme as _,
    input::EditorState,
    text::{RenderedText, TextViewState},
};
use gpui_kit::{TestSupportExt as _, prelude::FluentBuilder as _, *};

use crate::theme::{self, Palette, c, z};

/// Re-render only after typing pauses this long, so a long document does not re-render per key.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// Body text size at 100% zoom; headings below h2 keep gpui-kit's own ratios to it.
const BODY: f32 = 14.;
/// Top padding at 100% zoom; a line the preview follows lands this far below the top edge.
const TOP: f32 = 26.;
/// Frames a reveal may take to report its line before the preview stops waiting for it.
const REVEAL_FRAMES: u8 = 2;

pub struct Preview {
    state: Entity<TextViewState>,
    /// The Markdown editor shown; none when the active tab is not Markdown.
    source: Option<Entity<EditorState>>,
    task: Task<()>,
    scroll: ScrollHandle,
    /// The editor's top visible line, which the preview keeps at its own top.
    top_line: Option<usize>,
    /// `top_line` changed and the preview has not moved there yet.
    follow_pending: bool,
    /// The line the text view is revealing for the preview, until it reports where that line is.
    reveal: Rc<Cell<Option<Reveal>>>,
    _state: Subscription,
}

/// A line [`TextViewState::reveal_range`] was asked to find, and which of its edges goes to the top.
#[derive(Clone, Copy)]
struct Reveal {
    edge: Edge,
    /// Frames left in which the preview hides every line, see [`HideLines`].
    frames: u8,
}

#[derive(Clone, Copy)]
enum Edge {
    Top,
    Bottom,
}

/// Where the preview goes to follow an editor line.
enum Target {
    Start,
    End,
    /// A line of the rendered text, by an offset into it.
    Line {
        at: usize,
        edge: Edge,
    },
}

impl Preview {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| TextViewState::markdown("", cx));
        // A parse that lands may be the one a move of the editor was waiting for.
        let _state = cx.observe(&state, |this, _, cx| this.line_up(cx));
        Self {
            state,
            source: None,
            task: Task::ready(()),
            scroll: ScrollHandle::new(),
            top_line: None,
            follow_pending: false,
            reveal: Rc::default(),
            _state,
        }
    }

    /// Shows `source` now, dropping a pending update.
    pub fn show(&mut self, source: Option<Entity<EditorState>>, cx: &mut Context<Self>) {
        self.task = Task::ready(());
        if self.source != source {
            self.top_line = None;
            self.scroll.set_offset(Point::default());
        }
        self.source = source;
        self.update_text(cx);
        self.follow(cx);
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

    /// Follows the editor when its top visible line changed. Called after each frame: the editor
    /// learns which lines it shows while it paints, a frame after it scrolled.
    fn follow(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.source else {
            return;
        };
        let line = editor.read(cx).visible_row_range().map(|rows| rows.start);
        if line.is_none() || line == self.top_line {
            return;
        }
        self.top_line = line;
        self.follow_pending = true;
        self.line_up(cx);
    }

    /// Scrolls so the editor's top line is at the top, once the text view shows the editor's text.
    ///
    /// gpui-kit maps a source range to the rendered text, and `reveal_range` finds the line that
    /// text was laid out on; the reveal reports that line's bounds to `on_reveal`, which scrolls.
    fn line_up(&mut self, cx: &mut Context<Self>) {
        let (Some(editor), Some(line), true) = (&self.source, self.top_line, self.follow_pending)
        else {
            return;
        };
        let rendered = self.state.read(cx).rendered_text();
        // Until its parse lands the text view holds older text, whose lines are not the editor's.
        if rendered.source().len() != editor.read(cx).text().len() {
            return;
        }
        self.follow_pending = false;
        self.reveal.set(None);
        match target(&rendered, line) {
            Target::Start => self.scroll.set_offset(Point::default()),
            Target::End => self.scroll.scroll_to_bottom(),
            Target::Line { at, edge } => {
                let requested = self
                    .state
                    .update(cx, |state, cx| state.reveal_range(at..at, cx));
                if requested.is_ok() {
                    self.reveal.set(Some(Reveal {
                        edge,
                        frames: REVEAL_FRAMES,
                    }));
                }
            }
        }
        cx.notify();
    }

    /// Whether this frame hides every line from the reveal, counting the frame against its budget.
    fn take_hide_frame(&self) -> bool {
        match self.reveal.get() {
            Some(reveal) if reveal.frames > 0 => {
                self.reveal.set(Some(Reveal {
                    frames: reveal.frames - 1,
                    ..reveal
                }));
                true
            }
            _ => false,
        }
    }

    fn text_view(&self, p: &Palette, cx: &App) -> TextView {
        let scroll = self.scroll.clone();
        let reveal = self.reveal.clone();
        let top = z(TOP, cx);
        TextView::new(&self.state)
            .style(preview_style(p, cx))
            // Called while the revealed line is off screen; it lands at once, no animation.
            .on_reveal(move |line, window, _| {
                let Some(Reveal { edge, .. }) = reveal.take() else {
                    return;
                };
                let y = match edge {
                    Edge::Top => line.top(),
                    Edge::Bottom => line.bottom(),
                };
                let offset = scroll.offset();
                let max = scroll.max_offset().y;
                let goal = offset.y + scroll.bounds().top() + top - y;
                scroll.set_offset(point(offset.x, goal.clamp(-max, px(0.))));
                window.refresh();
            })
    }
}

/// Where the editor's line `line` (from 0) of the rendered text's source shows in the preview.
fn target(rendered: &RenderedText, line: usize) -> Target {
    if line == 0 {
        return Target::Start;
    }
    let source = rendered.source();
    let Some(range) = line_range(source, line) else {
        return Target::End;
    };
    // The line's own text, when it renders any, goes to the top.
    if let Some(text) = rendered.range_for_source(range.clone()) {
        return Target::Line {
            at: text.start,
            edge: Edge::Top,
        };
    }
    // A line that renders no text (blank, a fence, an image, a rule) is what follows the text
    // before it, so that text goes just above the top.
    match rendered.range_for_source(0..range.start) {
        Some(before) => Target::Line {
            at: rendered.as_str()[..before.end]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i),
            edge: Edge::Bottom,
        },
        None => Target::Start,
    }
}

/// The byte range of line `line` (from 0) of `text`, without its line break.
fn line_range(text: &str, line: usize) -> Option<Range<usize>> {
    let start = match line {
        0 => 0,
        _ => text.match_indices('\n').nth(line - 1)?.0 + 1,
    };
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    Some(start..end)
}

/// Lays out and paints its child as is, but can prepaint it under an empty content mask, which
/// makes every line count as off screen. gpui-kit reports where a revealed line is only when that
/// line is off screen; hiding them all for one frame makes it report a line in view too.
struct HideLines {
    child: AnyElement,
    hide: bool,
}

impl IntoElement for HideLines {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for HideLines {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.hide {
            let mask = ContentMask {
                bounds: Bounds::new(bounds.origin, size(bounds.size.width, px(0.))),
            };
            window.with_content_mask(Some(mask), |window| {
                self.child.prepaint(window, cx);
            });
        } else {
            self.child.prepaint(window, cx);
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.source.is_some() {
            cx.defer_in(window, |this, _, cx| this.follow(cx));
        }
        let p = theme::colors(cx);
        div()
            .id("preview")
            .track_scroll(&self.scroll)
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .bg(c(p.paper))
            .border_l_1()
            .border_color(c(p.line))
            .px(z(40., cx))
            .py(z(TOP, cx))
            .text_color(c(p.ink))
            .map(|el| {
                if self.source.is_some() {
                    let lines = HideLines {
                        child: self.text_view(p, cx).into_any_element(),
                        hide: self.take_hide_frame(),
                    };
                    el.text_size(z(BODY, cx))
                        .line_height(relative(1.7))
                        // Observed so the offscreen checks can see how far the page scrolled.
                        .child(div().id("preview-page").child(lines).test_support())
                } else {
                    el.text_size(z(13., cx))
                        .text_color(c(p.sub))
                        .child("Preview shows Markdown files")
                }
            })
            .test_support()
    }
}

#[cfg(test)]
mod tests {
    use super::line_range;

    #[test]
    fn line_range_finds_each_line_without_its_break() {
        let text = "# Title\n\nbody\r\nlast";
        let lines: Vec<_> = (0..4)
            .map(|i| line_range(text, i).map(|r| &text[r]))
            .collect();
        assert_eq!(
            lines,
            [Some("# Title"), Some(""), Some("body\r"), Some("last")]
        );
        assert_eq!(line_range(text, 4), None);
        assert_eq!(line_range("a\n", 1), Some(2..2));
    }
}
