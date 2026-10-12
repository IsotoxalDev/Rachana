use gpui::*;

use crate::ui::overlay::modal_overlay;
use crate::ui::text_field::TextField;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

pub struct PropertiesApplied {
    pub title: String,
    pub credit: String,
    pub author: String,
    pub source: String,
    pub draft_date: String,
    pub contact: String,
    pub notes: String,
}

// ---------------------------------------------------------------------------
// PropertiesPanel
// ---------------------------------------------------------------------------

const FIELD_LABELS: [&str; 7] = [
    "Title", "Credit", "Author", "Source", "Draft date", "Contact", "Notes",
];

const FIELD_PLACEHOLDERS: [&str; 7] = [
    "Untitled", "written by", "Your Name", "Based on the novel by...",
    "01/01/2026", "email or phone", "Additional notes",
];

pub struct PropertiesPanel {
    pub fields: Vec<Entity<TextField>>,
    focused: usize,
    focus_handle: FocusHandle,
}

impl EventEmitter<PropertiesApplied> for PropertiesPanel {}

impl Focusable for PropertiesPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl PropertiesPanel {
    pub fn new(
        title: &str, credit: &str, author: &str, source: &str,
        draft_date: &str, contact: &str, notes: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        // Notes may be multiline from the file; collapse to a single display line.
        let notes_display = notes.replace('\n', " · ");
        let values = [title, credit, author, source, draft_date, contact, notes_display.as_str()];
        let fields = values.iter().zip(FIELD_PLACEHOLDERS).map(|(v, placeholder)| {
            let v = v.to_string();
            cx.new(|cx| TextField::new(v, cx).with_placeholder(placeholder))
        }).collect();

        Self { fields, focused: 0, focus_handle: cx.focus_handle() }
    }

    pub fn first_focus_handle(&self, cx: &App) -> FocusHandle {
        self.fields[0].read(cx).focus_handle.clone()
    }

    fn focused_field_count(&self) -> usize { self.fields.len() }

    fn apply(&mut self, cx: &mut Context<Self>) {
        let vals: Vec<String> = self.fields.iter()
            .map(|f| f.read(cx).content.to_string())
            .collect();
        cx.emit(PropertiesApplied {
            title:      vals[0].clone(),
            credit:     vals[1].clone(),
            author:     vals[2].clone(),
            source:     vals[3].clone(),
            draft_date: vals[4].clone(),
            contact:    vals[5].clone(),
            notes:      vals[6].clone(),
        });
    }

    fn move_focus(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.focused_field_count();
        self.focused = if forward {
            (self.focused + 1) % n
        } else {
            (self.focused + n - 1) % n
        };
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
                self.apply(cx);
                cx.stop_propagation();
            }
            "enter" | "return" => {
                self.apply(cx);
                cx.stop_propagation();
            }
            // Plain characters are deliberately left to propagate: the platform's
            // text-input fallback (see gpui_linux Wayland `handle_input`) only
            // delivers typed characters to the focused text field's input handler
            // when the keystroke's propagation was never stopped. DocumentView's
            // own key handler is guarded by `properties_open` so it won't act on
            // these once they bubble past this panel.
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for PropertiesPanel {
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
            .w(px(420.0))
            .bg(rgb(0x2a2a2a))
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x484848))
            .shadow_lg()
            .on_key_down(cx.listener(Self::handle_panel_key))
            // Header
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
                            .child("Script Properties"),
                    ),
            )
            // Fields
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(fields_ui),
            )
            // Buttons
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
                            .id("props-cancel")
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.apply(cx);
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
                            .id("props-apply")
                            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                                this.apply(cx);
                                cx.stop_propagation();
                            }))
                            .child("Apply"),
                    ),
            );

        modal_overlay(0.45, panel, cx)
    }
}
