//! The window's title bar: macOS traffic lights, then the folder name and the active file.

use gpui_kit::*;

use crate::theme::{self, c, size, z};

/// Where macOS draws the traffic lights inside subwy's own title bar.
pub fn traffic_lights() -> Point<Pixels> {
    point(px(14.), px(13.))
}

pub fn render(folder: Option<&str>, file: Option<&str>, cx: &App) -> impl IntoElement {
    let p = theme::colors(cx);
    div()
        .id("title-bar")
        .h(z(size::TITLE_BAR, cx))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        // On the desk with the islands below it, so no line.
        .bg(c(p.desk))
        .window_control_area(WindowControlArea::Drag)
        .on_click(|event, window, _| {
            if event.click_count() == 2 {
                window.titlebar_double_click();
            }
        })
        .text_size(z(size::UI_TEXT, cx))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(c(p.ink))
                .child(folder.unwrap_or("subwy").to_string()),
        )
        .children(file.map(|file| div().ml_2().text_color(c(p.sub)).child(file.to_string())))
}
