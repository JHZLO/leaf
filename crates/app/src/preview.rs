//! The Markdown preview beside the editor (Cmd+Shift+V).

use std::time::Duration;

use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::*;

use crate::theme::{self, c};

/// Re-render only after typing pauses this long, so a long document does not re-render per key.
const DEBOUNCE: Duration = Duration::from_millis(150);

pub struct Preview {
    state: Entity<TextViewState>,
    task: Task<()>,
}

impl Preview {
    pub fn new(source: &str, cx: &mut Context<Self>) -> Self {
        Self {
            state: cx.new(|cx| TextViewState::markdown(source, cx)),
            task: Task::ready(()),
        }
    }

    pub fn schedule(&mut self, source: String, cx: &mut Context<Self>) {
        self.task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                this.state
                    .update(cx, |state, cx| state.set_text(&source, cx));
                cx.notify();
            });
        });
    }
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
            .px_10()
            .py(px(26.))
            .child(TextView::new(&self.state))
    }
}
