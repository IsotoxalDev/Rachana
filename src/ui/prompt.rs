use gpui::*;

pub struct PromptRenderer {
    message: String,
    detail: Option<String>,
    actions: Vec<PromptButton>,
    focus: FocusHandle,
}

impl EventEmitter<PromptResponse> for PromptRenderer {}

impl Focusable for PromptRenderer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PromptRenderer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog = div()
            .cursor_default()
            .track_focus(&self.focus)
            .w(px(380.0))
            .bg(rgb(0x2a2a2a))
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x484848))
            .shadow_lg()
            .p(px(20.0))
            // Title
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(hsla(0.0, 0.0, 0.95, 1.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .mb(px(8.0))
                    .child(self.message.clone()),
            )
            // Detail
            .children(self.detail.clone().map(|detail| {
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(0.0, 0.0, 0.65, 1.0))
                    .mb(px(20.0))
                    .child(detail)
            }))
            // Buttons row
            .child({
                let actions = self.actions.clone();
                let mut row = div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(8.0));
                for (ix, action) in actions.into_iter().enumerate() {
                    let base = div()
                        .px(px(14.0))
                        .py(px(6.0))
                        .text_size(px(12.0))
                        .rounded_md()
                        .cursor_pointer()
                        .id(ix)
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.emit(PromptResponse(ix));
                            cx.stop_propagation();
                        }));

                    let styled = if ix == 0 {
                        base.bg(rgb(0x4a90d9))
                            .text_color(hsla(0.0, 0.0, 1.0, 1.0))
                            .hover(|s| s.bg(rgb(0x3a7ec5)))
                    } else {
                        base.bg(rgb(0x383838))
                            .text_color(hsla(0.0, 0.0, 0.85, 1.0))
                            .border_1()
                            .border_color(rgb(0x555555))
                            .hover(|s| s.bg(rgb(0x444444)))
                    };

                    row = row.child(styled.child(action.label().clone()));
                }
                row
            });

        div()
            .size_full()
            // Backdrop
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(hsla(0.0, 0.0, 0.0, 0.5)),
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
}

pub fn build_prompt(
    _level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    actions: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let view = cx.new(|cx| PromptRenderer {
        message: message.to_string(),
        detail: detail.map(ToString::to_string),
        actions: actions.to_vec(),
        focus: cx.focus_handle(),
    });
    handle.with_view(view, window, cx)
}
