use gpui::*;

use crate::ui::overlay::modal_overlay;
use crate::ui::text_field::TextField;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

pub struct NewFranchiseCreated {
    pub name: String,
}

pub struct NewFranchiseCancelled;

// ---------------------------------------------------------------------------
// NewFranchiseDialog — names a new folder-like grouping on the Films library.
// ---------------------------------------------------------------------------

pub struct NewFranchiseDialog {
    name_field: Entity<TextField>,
    focus_handle: FocusHandle,
}

impl EventEmitter<NewFranchiseCreated> for NewFranchiseDialog {}
impl EventEmitter<NewFranchiseCancelled> for NewFranchiseDialog {}

impl Focusable for NewFranchiseDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl NewFranchiseDialog {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let name_field = cx.new(|cx| {
            TextField::new(String::new(), cx).with_placeholder("Series or franchise name")
        });
        Self { name_field, focus_handle: cx.focus_handle() }
    }

    pub fn first_focus_handle(&self, cx: &App) -> FocusHandle {
        self.name_field.read(cx).focus_handle.clone()
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let name = self.name_field.read(cx).content.trim().to_string();
        if !name.is_empty() {
            cx.emit(NewFranchiseCreated { name });
        } else {
            cx.emit(NewFranchiseCancelled);
        }
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(NewFranchiseCancelled);
    }

    fn handle_panel_key(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
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

impl Render for NewFranchiseDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.name_field.read(cx).focus_handle.is_focused(window);
        let border_color = if is_focused { rgb(0x4a90d9) } else { rgb(0x444444) };

        let panel = div()
            .w(px(360.0))
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
                            .child("New Franchise"),
                    ),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(hsla(0.0, 0.0, 0.55, 1.0))
                            .child("Name"),
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
                            .child(self.name_field.clone()),
                    ),
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
                            .id("new-franchise-cancel")
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
                            .id("new-franchise-create")
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
