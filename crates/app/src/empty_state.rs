//! What a window shows before a folder is open (design doc 5.8).

use std::path::PathBuf;

use gpui_kit::{prelude::FluentBuilder as _, *};
use subwy_project::paths;

use crate::{
    keymap::OpenFolder,
    theme::{self, c, z},
};

/// The subwy mark (assets/brand/mark.svg): the first three lines of the tree's route crossing at
/// two interchanges, with the station at the end of the first line.
pub fn subwy_mark(side: Pixels, cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    let [first, second, third] = [p.route[0], p.route[1], p.route[2]].map(c);
    let (ink, paper) = (c(p.ink), c(p.paper));
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let unit = bounds.size.width / 100.;
            let at = |x: f32, y: f32| point(bounds.origin.x + unit * x, bounds.origin.y + unit * y);
            let dot = |window: &mut Window, (x, y): (f32, f32), r: f32, color: Hsla| {
                let r = unit * r;
                let center = at(x, y);
                let corner = point(center.x - r, center.y - r);
                window.paint_quad(
                    fill(Bounds::new(corner, size(r * 2., r * 2.)), color).corner_radii(r),
                );
            };
            // Each line, with round ends like the SVG's.
            let lines: [(&[(f32, f32)], Hsla); 3] = [
                (&[(50., 12.), (50., 88.)], second),
                (&[(18., 76.), (82., 76.)], third),
                (&[(12., 62.), (36., 62.), (62., 36.), (82., 36.)], first),
            ];
            for (points, color) in lines {
                let mut path = PathBuilder::stroke(unit * 8.);
                path.move_to(at(points[0].0, points[0].1));
                for &(x, y) in &points[1..] {
                    path.line_to(at(x, y));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, color);
                }
                for &end in [points[0], points[points.len() - 1]].iter() {
                    dot(window, end, 4., color);
                }
            }
            for ring in [(50., 48.), (50., 76.)] {
                let r = unit * 7.;
                let center = at(ring.0, ring.1);
                window.paint_quad(quad(
                    Bounds::new(point(center.x - r, center.y - r), size(r * 2., r * 2.)),
                    r,
                    paper,
                    unit * 3.,
                    ink,
                    BorderStyle::Solid,
                ));
            }
            dot(window, (82., 36.), 10.5, first.opacity(0.22));
            dot(window, (82., 36.), 6.5, first);
        },
    )
    .size(side)
}

/// The recent folders: name, then where it lives; Up/Down choose, Enter or a click opens.
fn recent_list(
    recent: &[PathBuf],
    selected: usize,
    on_open: impl Fn(usize, &mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let p = theme::colors(cx);
    div()
        .mt(z(18., cx))
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .px(z(10., cx))
                .pb(z(6., cx))
                .text_size(z(12., cx))
                .text_color(c(p.sub))
                .child("Recent"),
        )
        .children(
            recent
                .iter()
                .take(SHOWN_RECENT)
                .enumerate()
                .map(|(i, root)| {
                    let chosen = i == selected;
                    let name = root
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let place = root.parent().map(paths::tilde).unwrap_or_default();
                    let on_open = on_open.clone();
                    div()
                        .id(("recent", i))
                        .h(z(28., cx))
                        .px(z(10., cx))
                        .flex()
                        .items_center()
                        .gap(z(8., cx))
                        .rounded(z(4., cx))
                        .text_size(z(13., cx))
                        .text_color(c(if chosen { p.sel_ink } else { p.ink }))
                        .when(chosen, |el| el.bg(c(p.sel_bg)))
                        .when(!chosen, |el| el.hover(|style| style.bg(c(p.hover))))
                        .child(div().flex_none().child(name))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(z(12., cx))
                                .text_color(c(if chosen { p.sel_ink } else { p.sub }))
                                .child(place),
                        )
                        .on_click(move |_, window, cx| on_open(i, window, cx))
                }),
        )
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

/// How many recent folders the empty window lists.
pub const SHOWN_RECENT: usize = 5;

/// `recent` are folders opened before (newest first); `selected` is the keyboard choice among them;
/// `on_open` opens the one at an index.
pub fn render(
    recent: &[PathBuf],
    selected: usize,
    on_open: impl Fn(usize, &mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
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
                .child(subwy_mark(z(48., cx), cx))
                .child(
                    div()
                        .text_size(z(22., cx))
                        .font_weight(FontWeight::MEDIUM)
                        .child("subwy"),
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
                .when(!recent.is_empty(), |el| {
                    el.child(recent_list(recent, selected, on_open, cx))
                })
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
