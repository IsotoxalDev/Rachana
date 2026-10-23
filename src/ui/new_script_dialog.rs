use gpui::*;

use crate::ui::overlay::modal_overlay;
use crate::ui::text_field::TextField;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

pub struct NewScriptCreated {
    pub title: String,
    pub author: String,
}

pub struct NewScriptCancelled;

// ---------------------------------------------------------------------------
// NewScriptDialog — collects title/author before a new script opens.
// Franchise/series assignment happens afterward, from the Films library.
// ---------------------------------------------------------------------------

const FIELD_LABELS: [&str; 2] = ["Title", "Author"];
const FIELD_PLACEHOLDERS: [&str; 2] = ["Untitled", "Your Name"];

pub struct NewScriptDialog {
    fields: Vec<Entity<TextField>>,
    focused: usize,
    focus_handle: FocusHandle,
}

impl EventEmitter<NewScriptCreated> for NewScriptDialog {}
impl EventEmitter<NewScriptCancelled> for NewScriptDialog {}

impl Focusable for NewScriptDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl NewScriptDialog {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let fields = FIELD_PLACEHOLDERS.iter().map(|placeholder| {
            cx.new(|cx| TextField::new(String::new(), cx).with_placeholder(*placeholder))
        }).collect();

        Self { fields, focused: 0, focus_handle: cx.focus_handle() }
    }

    pub fn first_focus_handle(&self, cx: &App) -> FocusHandle {
        self.fields[0].read(cx).focus_handle.clone()
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let title = self.fields[0].read(cx).content.trim().to_string();
        let author = self.fields[1].read(cx).content.trim().to_string();
        cx.emit(NewScriptCreated { title, author });
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(NewScriptCancelled);
    }

    fn move_focus(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.fields.len();
        self.focused = if forward { (self.focused + 1) % n } else { (self.focused + n - 1) % n };
        let handle = self.fields[self.focused].read(cx).focus_handle.clone();
        handle.focus(window, cx);
    }

    fn handle_panel_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "tab" => {
                self.move_focus(!event.keystroke.modifiers.shift, window, cx);
                cx.stop_propagation();
            }
            "escape" => {
                self.cancel(cx);
                cx.stop_propagation();
            }
            "enter" | "return" => {
                self.create(cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for NewScriptDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fields_ui: Vec<_> = self.fields.iter().enumerate().map(|(i, field_entity)| {
            let is_focused = field_entity.read(cx).focus_handle.is_focused(window);
            let border_color = if is_focused { rgb(0x4a90d9) } else { rgb(0x444444) };

            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(hsla(0.0, 0.0, 0.55, 1.0))
                        .child(FIELD_LABELS[i]),
                )
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(6.0))
                        .bg(rgb(0x1e1e1e))
                        .rounded_md()
                        .border_1()
                        .border_color(border_color)
                        .h(px(32.0))
                        .child(field_entity.clone()),
                )
        }).collect();

        let panel = div()
            .w(px(380.0))
            .bg(rgb(0x2a2a2a))
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x484848))
            .shadow_lg()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_panel_key))
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .border_b_1()
                    .border_color(rgb(0x3a3a3a))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(hsla(0.0, 0.0, 0.9, 1.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("New Script"),
                    ),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(fields_ui),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .border_t_1()
                    .border_color(rgb(0x3a3a3a))
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        div()
                            .px(px(14.0))
                            .py(px(6.0))
                            .text_size(px(12.0))
                            .text_color(hsla(0.0, 0.0, 0.85, 1.0))
                            .bg(rgb(0x383838))
                            .border_1()
                            .border_color(rgb(0x555555))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x444444)))
                            .id("new-script-cancel")
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.cancel(cx);
                                cx.stop_propagation();
                            }))
                            .child("Cancel"),
                    )
                    .child(
                        div()
                            .px(px(14.0))
                            .py(px(6.0))
                            .text_size(px(12.0))
                            .text_color(hsla(0.0, 0.0, 1.0, 1.0))
                            .bg(rgb(0x4a90d9))
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x3a7ec5)))
                            .id("new-script-create")
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.create(cx);
                                cx.stop_propagation();
                            }))
                            .child("Create"),
                    ),
            );

        modal_overlay(0.55, panel, cx)
    }
}
