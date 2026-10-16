//! What a window shows before a folder is open (design doc 5.8).

use gpui_kit::*;

use crate::{
    keymap::OpenFolder,
    theme::{self, c, z},
};

/// The leaf mark: a leaf outline with its midrib, drawn in `color` on a 48-unit grid.
pub fn leaf_mark(size: Pixels, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let unit = bounds.size.width / 48.;
            let at = |x: f32, y: f32| point(bounds.origin.x + unit * x, bounds.origin.y + unit * y);
            let mut outline = PathBuilder::stroke(unit * 2.2);
            outline.move_to(at(12., 36.));
            outline.cubic_bezier_to(at(38., 10.), at(12., 20.), at(22., 10.));
            outline.cubic_bezier_to(at(12., 36.), at(38., 26.), at(28., 36.));
            outline.close();
            let mut midrib = PathBuilder::stroke(unit * 2.2);
            midrib.move_to(at(12., 36.));
            midrib.line_to(at(28., 20.));
            for path in [outline.build(), midrib.build()].into_iter().flatten() {
                window.paint_path(path, color);
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
                .child(leaf_mark(z(48., cx), c(p.accent)))
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
