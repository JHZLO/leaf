//! The islands: the tree, the editor and the terminal sit apart on the desk, and dragging the gap
//! between two of them resizes them.

use gpui_kit::{AnyWindowHandle, Modifiers, MouseButton, Pixels, Point, point, px};

use crate::check::{Check, fixture};

/// The gap and the sizes the islands start with (theme.rs, at 100% zoom).
const GAP: f32 = 6.;
const TREE: f32 = 300.;
const TERMINAL: f32 = 260.;
const STATUS_BAR: f32 = 26.;

pub fn run(check: &mut Check) {
    let fixture = fixture();
    let window = check.open(Some(fixture.path()));
    check.shot(window, "islands-1");
    let height = check
        .cx
        .update_window(window, |_, window, _| {
            window.viewport_size().height.as_f32()
        })
        .expect("window");

    // The gap right of the tree, 100 px to the right.
    let x = GAP + TREE + GAP / 2.;
    drag(
        check,
        window,
        point(px(x), px(300.)),
        point(px(x + 100.), px(300.)),
    );
    let (tree, _) = check.workspace(window, |w, cx| w.island_sizes(cx));
    check.expect_eq(
        "dragging the gap widens the tree",
        tree.map(f32::round),
        Some(400.),
    );

    // The gap above the terminal, 100 px up.
    let y = height - STATUS_BAR - GAP - TERMINAL - GAP / 2.;
    drag(
        check,
        window,
        point(px(700.), px(y)),
        point(px(700.), px(y - 100.)),
    );
    let (_, terminal) = check.workspace(window, |w, cx| w.island_sizes(cx));
    check.expect_eq(
        "dragging the gap raises the terminal",
        terminal.round(),
        360.,
    );
    check.shot(window, "islands-2-resized");
}

/// Presses at `from`, moves past GPUI's drag threshold, then to `to`, and lets go.
fn drag(check: &mut Check, window: AnyWindowHandle, from: Point<Pixels>, to: Point<Pixels>) {
    let none = Modifiers::default();
    check.cx.simulate_mouse_move(window, from, None, none);
    check
        .cx
        .simulate_mouse_down(window, from, MouseButton::Left, none);
    let nudge = point(
        from.x + (to.x - from.x) * 0.1,
        from.y + (to.y - from.y) * 0.1,
    );
    check
        .cx
        .simulate_mouse_move(window, nudge, MouseButton::Left, none);
    check
        .cx
        .simulate_mouse_move(window, to, MouseButton::Left, none);
    check
        .cx
        .simulate_mouse_up(window, to, MouseButton::Left, none);
    check.cx.run_until_parked();
}
