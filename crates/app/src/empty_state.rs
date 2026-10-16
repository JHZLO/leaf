//! What a window shows before a folder is open (design doc 5.8).

use gpui_kit::*;

use crate::{
    keymap::OpenFolder,
    theme::{self, c, z},
};

/// The leaf mark (assets/brand/mark.svg): a leaf whose veins are the tree's bent guides.
/// Below 24 px the side veins would blur, so only the midrib stays (mark-small.svg).
pub fn leaf_mark(size: Pixels, fill: Hsla, stroke: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let unit = bounds.size.width / 100.;
            let small = bounds.size.width < px(24.);
            let width = unit * if small { 7. } else { 4.5 };
            let at = |x: f32, y: f32| point(bounds.origin.x + unit * x, bounds.origin.y + unit * y);
            let blade = |mut path: PathBuilder| {
                path.move_to(at(50., 14.));
                path.cubic_bezier_to(at(64., 74.), at(72., 30.), at(78., 54.));
                path.cubic_bezier_to(at(36., 74.), at(58., 82.), at(42., 82.));
                path.cubic_bezier_to(at(50., 14.), at(22., 54.), at(28., 30.));
                path.close();
                path
            };
            let mut veins = PathBuilder::stroke(width);
            veins.move_to(at(50., if small { 30. } else { 26. }));
            veins.line_to(at(50., 88.));
            if !small {
                for (y, x) in [(38., 59.), (52., 41.), (64., 58.)] {
                    veins.move_to(at(50., y));
                    veins.line_to(at(x, y));
                }
            }
            if let Ok(path) = blade(PathBuilder::fill()).build() {
                window.paint_path(path, fill);
            }
            for path in [blade(PathBuilder::stroke(width)).build(), veins.build()]
                .into_iter()
                .flatten()
            {
                window.paint_path(path, stroke);
            }
        },
    )
    .size(size)
}

fn keycap(key: &'static str, cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    div()
        .px(z(5., cx))
        .rounded(px(4.))
        .border_1()
        .border_color(c(p.line))
        .bg(c(p.paper))
        .text_size(z(11., cx))
        .text_color(c(p.ink))
        .child(key)
}

pub fn render(cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    let hint = |key: &'static str, label: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(z(14., cx))
            .child(
                div()
                    .w(z(48., cx))
                    .flex()
                    .justify_end()
                    .child(keycap(key, cx)),
            )
            .child(label)
    };
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .bg(c(p.paper))
        .child(
            div()
                .w(z(380., cx))
                .flex()
                .flex_col()
                .items_center()
                .gap(z(12., cx))
                .child(leaf_mark(z(48., cx), c(p.sel_bg), c(p.accent)))
                .child(
                    div()
                        .text_size(z(22., cx))
                        .font_weight(FontWeight::MEDIUM)
                        .child("leaf"),
                )
                .child(
                    div()
                        .text_size(z(13., cx))
                        .text_color(c(p.sub))
                        .child("Open a folder to walk its tree."),
                )
                .child(
                    div()
                        .id("open-folder")
                        .mt(z(10., cx))
                        .h(z(34., cx))
                        .pl(z(16., cx))
                        .pr(z(14., cx))
                        .flex()
                        .items_center()
                        .gap(z(10., cx))
                        .rounded(z(6., cx))
                        .border_1()
                        .border_color(c(p.line))
                        .bg(c(p.side))
                        .hover(|style| style.bg(c(p.hover)))
                        .text_size(z(13., cx))
                        .child("Open folder")
                        .child(keycap("⌘O", cx))
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenFolder), cx)),
                )
                .child(
                    div()
                        .mt(z(22., cx))
                        .flex()
                        .flex_col()
                        .gap(z(8., cx))
                        .text_size(z(12., cx))
                        .text_color(c(p.sub))
                        .child(hint("⌘1", "Focus the tree"))
                        .child(hint("⌘⇧C", "Copy path"))
                        .child(hint("⌘=", "Zoom in")),
                ),
        )
}
