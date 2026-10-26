use gpui::*;

/// Full-window backdrop + centered dialog wrapper shared by modal-style
/// panels (prompts, properties, etc). Swallows backdrop clicks and scroll
/// so neither leaks through to whatever is scrollable underneath.
pub fn modal_overlay<T: Render>(
    backdrop_opacity: f32,
    dialog: impl IntoElement,
    cx: &mut Context<T>,
) -> Div {
    div()
        .size_full()
        .absolute()
        .top_0()
        .left_0()
        .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.stop_propagation()))
        .on_mouse_down(MouseButton::Right, cx.listener(|_, _, _, cx| cx.stop_propagation()))
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        // Backdrop
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(hsla(0.0, 0.0, 0.0, backdrop_opacity)),
        )
        // Centered dialog
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(dialog),
        )
}
