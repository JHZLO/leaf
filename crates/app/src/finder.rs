//! The file finder (Cmd+Shift+O): type part of a path, pick a file, open it.

use std::{ops::Range, path::PathBuf, sync::Arc};

use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_kit::{prelude::FluentBuilder as _, *};
use subwy_project::{
    folders::Folders,
    fuzzy,
    index::{self, FileIndex},
};

use crate::theme::{self, c, size, z};

/// Sizes at 100% zoom.
const WIDTH: f32 = 560.;
/// Below the title bar, over the top of the tab bar.
const TOP_GAP: f32 = 8.;
const INPUT_HEIGHT: f32 = 34.;
const INPUT_LINE: f32 = 20.;
const ROW_HEIGHT: f32 = 24.;
const PADDING_X: f32 = 12.;
const LIST_PADDING_Y: f32 = 4.;
const NAME_GAP: f32 = 8.;
const FOLDER_TEXT: f32 = 12.;
const RADIUS: f32 = 6.;

/// The most results shown.
const MAX_ROWS: usize = 12;

pub enum FinderEvent {
    /// Enter or a click picked this file, relative to the open folder.
    Open(PathBuf),
    /// Escape: close and give focus back to where it was.
    Dismiss,
    /// Focus went elsewhere (a click outside, Cmd+1): close and leave it there.
    FocusLost,
}

pub struct Finder {
    input: Entity<InputState>,
    /// Empty until the walk of the folder, started when the finder opens, comes back.
    index: Option<Arc<FileIndex>>,
    matches: Vec<fuzzy::Match>,
    selected: usize,
    _load: Task<()>,
    /// The search for the latest query; replacing it drops the result of an older one.
    search: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<FinderEvent> for Finder {}

impl Focusable for Finder {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Finder {
    pub fn new(folders: Folders, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Go to file"));
        let subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => this.refilter(cx),
                InputEvent::Blur => cx.emit(FinderEvent::FocusLost),
                _ => {}
            },
        )];
        let load = cx.spawn(async move |this, cx| {
            let index = cx
                .background_spawn(async move { index::build_folders(&folders) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.index = Some(Arc::new(index));
                this.refilter(cx);
            });
        });
        Self {
            input,
            index: None,
            matches: Vec::new(),
            selected: 0,
            _load: load,
            search: Task::ready(()),
            _subscriptions: subscriptions,
        }
    }

    /// Searches off the main thread: a large folder takes a few milliseconds per keystroke.
    fn refilter(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.index.clone() else {
            return;
        };
        let query = self.input.read(cx).value().to_string();
        let search =
            cx.background_spawn(async move { fuzzy::search(&query, &index.paths, MAX_ROWS) });
        self.search = cx.spawn(async move |this, cx| {
            let matches = search.await;
            let _ = this.update(cx, |this, cx| {
                this.matches = matches;
                this.selected = 0;
                cx.notify();
            });
        });
    }

    /// Up and Down stop at the ends, as in the tree.
    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(last) = self.matches.len().checked_sub(1) else {
            return;
        };
        self.selected = self.selected.saturating_add_signed(delta).min(last);
        cx.notify();
    }

    fn open(&mut self, row: usize, cx: &mut Context<Self>) {
        let (Some(index), Some(found)) = (&self.index, self.matches.get(row)) else {
            return;
        };
        cx.emit(FinderEvent::Open(PathBuf::from(&index.paths[found.index])));
    }

    fn render_row(
        &self,
        row: usize,
        path: &str,
        found: &fuzzy::Match,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let p = theme::colors(cx);
        let selected = row == self.selected;
        let name_start = path.rfind('/').map_or(0, |slash| slash + 1);
        let folder = &path[..name_start.saturating_sub(1)];
        let name = &path[name_start..];
        let (folder_marks, name_marks) = matched_ranges(path, &found.positions, name_start);
        // Semibold: medium barely shows at 13 px.
        let matched = HighlightStyle {
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        let styled = |text: &str, marks: Vec<Range<usize>>| {
            StyledText::new(text.to_string())
                .with_highlights(marks.into_iter().map(|range| (range, matched)))
        };
        div()
            .id(("finder-row", row))
            .h(z(ROW_HEIGHT, cx))
            .px(z(PADDING_X, cx))
            .flex()
            .items_center()
            .gap(z(NAME_GAP, cx))
            .whitespace_nowrap()
            .overflow_hidden()
            .when(selected, |el| el.bg(c(p.sel_bg)))
            .when(!selected, |el| el.hover(|style| style.bg(c(p.hover))))
            .on_click(cx.listener(move |this, _, _, cx| this.open(row, cx)))
            .child(
                div()
                    .flex_none()
                    .text_color(c(if selected { p.sel_ink } else { p.ink }))
                    .child(styled(name, name_marks)),
            )
            .when(!folder.is_empty(), |el| {
                // A long folder loses its start, keeping the folders nearest the file.
                el.child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis_start()
                        .text_size(z(FOLDER_TEXT, cx))
                        .text_color(c(p.sub))
                        .child(styled(folder, folder_marks)),
                )
            })
    }
}

/// The matched characters as byte ranges, split into the folder's and the file name's (each
/// relative to its own text). A matched `/` between the two shows in neither.
fn matched_ranges(
    path: &str,
    positions: &[usize],
    name_start: usize,
) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let (mut folder, mut name) = (Vec::new(), Vec::new());
    let mut positions = positions.iter().peekable();
    for (i, (byte, ch)) in path.char_indices().enumerate() {
        if positions.next_if_eq(&&i).is_none() {
            continue;
        }
        let end = byte + ch.len_utf8();
        if byte >= name_start {
            name.push(byte - name_start..end - name_start);
        } else if end < name_start {
            folder.push(byte..end);
        }
    }
    (folder, name)
}

/// The finder placed at the top center of the window, over the tabs.
pub fn overlay(finder: Entity<Finder>, cx: &App) -> impl IntoElement {
    div()
        .absolute()
        .top(z(size::TITLE_BAR + TOP_GAP, cx))
        .left_0()
        .right_0()
        .flex()
        .justify_center()
        .child(finder)
}

impl Render for Finder {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = theme::colors(cx);
        let rows: Vec<Stateful<Div>> = match self.index.clone() {
            Some(index) => self
                .matches
                .iter()
                .enumerate()
                .map(|(row, found)| self.render_row(row, &index.paths[found.index], found, cx))
                .collect(),
            None => Vec::new(),
        };
        let input = Input::new(&self.input)
            .appearance(false)
            .px(z(PADDING_X, cx))
            .text_size(z(size::UI_TEXT, cx))
            .line_height(z(INPUT_LINE, cx));
        // Input's own `h` sizes only multi-line inputs; the style's height sizes a single line.
        let input = Styled::h(input, z(INPUT_HEIGHT, cx));
        div()
            .id("finder")
            // Clicks inside stay here: the window behind would otherwise take focus and close it.
            .occlude()
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &Enter, _, cx| this.open(this.selected, cx)))
            .on_action(cx.listener(|_, _: &Escape, _, cx| cx.emit(FinderEvent::Dismiss)))
            .w(z(WIDTH, cx))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(c(p.paper))
            .border_1()
            .border_color(c(p.line))
            .rounded(z(RADIUS, cx))
            .text_size(z(size::UI_TEXT, cx))
            .text_color(c(p.ink))
            .child(
                div()
                    .when(!rows.is_empty(), |el| {
                        el.border_b_1().border_color(c(p.line))
                    })
                    .child(input),
            )
            .when(!rows.is_empty(), |el| {
                el.child(div().py(z(LIST_PADDING_Y, cx)).children(rows))
            })
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: GPUI's glob export includes a `test` macro that would replace the built-in one.
    use super::matched_ranges;

    #[test]
    fn matched_characters_split_between_folder_and_name() {
        // "docs/guide/keys.md" with d, g, /, k, s matched.
        let (folder, name) = matched_ranges("docs/guide/keys.md", &[0, 5, 10, 11, 14], 11);
        assert_eq!(folder, [0..1, 5..6]);
        assert_eq!(name, [0..1, 3..4]);
    }

    #[test]
    fn matched_ranges_are_in_bytes() {
        let path = "\u{d55c}/\u{ae00}.md";
        let (folder, name) = matched_ranges(path, &[0, 2], 4);
        assert_eq!(folder, vec![0..3]);
        assert_eq!(name, vec![0..3]);
    }
}
