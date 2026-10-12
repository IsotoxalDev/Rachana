use std::ops::Range;
use gpui::*;

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

actions!(text_field, [
    Backspace, Delete, Left, Right,
    SelectLeft, SelectRight, SelectAll,
    Home, End, Paste, Cut, Copy,
]);

// ---------------------------------------------------------------------------
// TextField entity
// ---------------------------------------------------------------------------

pub struct TextField {
    pub focus_handle: FocusHandle,
    pub content: SharedString,
    pub placeholder: SharedString,
    pub selected_range: Range<usize>,
    pub selection_reversed: bool,
    pub marked_range: Option<Range<usize>>,
    pub last_layout: Option<ShapedLine>,
    pub last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

impl TextField {
    pub fn new(content: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        let content: SharedString = content.into();
        let len = content.len();
        Self {
            focus_handle: cx.focus_handle(),
            content,
            placeholder: "".into(),
            selected_range: len..len,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn set_content(&mut self, content: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = content.into();
        let len = self.content.len();
        self.selected_range = len..len;
        self.marked_range = None;
        cx.notify();
    }

    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed { self.selected_range.start } else { self.selected_range.end }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content[..offset].char_indices().next_back().map(|(i, _)| i).unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let mut it = self.content[offset..].char_indices();
        it.next();
        it.next().map(|(i, _)| offset + i).unwrap_or(self.content.len())
    }

    pub fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let (Some(bounds), Some(line)) = (&self.last_bounds, &self.last_layout) else { return 0; };
        if position.y < bounds.top() { return 0; }
        if position.y > bounds.bottom() { return self.content.len(); }
        line.closest_index_for_x(position.x - bounds.left())
    }

    // -- Actions --

    pub fn action_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let prev = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == prev { return; }
            self.select_to(prev, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub fn action_delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next { return; }
            self.select_to(next, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    pub fn action_left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let p = self.previous_boundary(self.cursor_offset());
            self.move_to(p, cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    pub fn action_right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let n = self.next_boundary(self.cursor_offset());
            self.move_to(n, cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    pub fn action_select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let p = self.previous_boundary(self.cursor_offset());
        self.select_to(p, cx);
    }

    pub fn action_select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let n = self.next_boundary(self.cursor_offset());
        self.select_to(n, cx);
    }

    pub fn action_select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    pub fn action_home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    pub fn action_end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    pub fn action_paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    pub fn action_copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    pub fn action_cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.action_copy(&Copy, window, cx);
        if !self.selected_range.is_empty() {
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.clone().focus(window, cx);
        self.is_selecting = true;
        if ev.modifiers.shift {
            self.select_to(self.index_for_mouse_position(ev.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(ev.position), cx);
        }
        cx.stop_propagation();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(ev.position), cx);
        }
    }
}

// ---------------------------------------------------------------------------
// EntityInputHandler (IME / platform text input)
// ---------------------------------------------------------------------------

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self, range_utf16: Range<usize>, actual_range: &mut Option<Range<usize>>,
        _: &mut Window, _: &mut Context<Self>,
    ) -> Option<String> {
        let range = range_from_utf16(&self.content, &range_utf16);
        actual_range.replace(range_to_utf16(&self.content, &range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self, _: bool, _: &mut Window, _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: range_to_utf16(&self.content, &self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| range_to_utf16(&self.content, r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self, range_utf16: Option<Range<usize>>, text: &str,
        _: &mut Window, cx: &mut Context<Self>,
    ) {
        let range = range_utf16.as_ref()
            .map(|r| range_from_utf16(&self.content, r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let new = format!("{}{}{}", &self.content[..range.start], text, &self.content[range.end..]);
        self.content = new.into();
        let end = range.start + text.len();
        self.selected_range = end..end;
        self.marked_range = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self, range_utf16: Option<Range<usize>>, new_text: &str,
        new_sel_utf16: Option<Range<usize>>, _: &mut Window, cx: &mut Context<Self>,
    ) {
        let range = range_utf16.as_ref()
            .map(|r| range_from_utf16(&self.content, r))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let new = format!("{}{}{}", &self.content[..range.start], new_text, &self.content[range.end..]);
        self.content = new.into();
        self.marked_range = if new_text.is_empty() { None }
            else { Some(range.start..range.start + new_text.len()) };
        self.selected_range = new_sel_utf16.as_ref()
            .map(|r| range_from_utf16(&self.content, r))
            .map(|r| range.start + r.start..range.start + r.end)
            .unwrap_or_else(|| { let e = range.start + new_text.len(); e..e });
        cx.notify();
    }

    fn bounds_for_range(
        &mut self, range_utf16: Range<usize>, bounds: Bounds<Pixels>,
        _: &mut Window, _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.last_layout.as_ref()?;
        let range = range_from_utf16(&self.content, &range_utf16);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(
        &mut self, pt: Point<Pixels>, _: &mut Window, _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let line = self.last_layout.as_ref()?;
        let local = bounds.localize(&pt)?;
        let idx = line.index_for_x(local.x)?;
        Some(offset_to_utf16(&self.content, idx))
    }
}

// ---------------------------------------------------------------------------
// Focusable + Render
// ---------------------------------------------------------------------------

impl Focusable for TextField {
    fn focus_handle(&self, _: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Render for TextField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("TextField")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            // Match the fixed content height of the field wrapper in properties.rs
            // (h(32) - py(6)*2 = 20px). Without this, the default TextStyle line
            // height (~26px, font_size * phi()) overflows the box.
            .text_size(px(13.0))
            .line_height(px(20.0))
            .on_action(cx.listener(Self::action_backspace))
            .on_action(cx.listener(Self::action_delete))
            .on_action(cx.listener(Self::action_left))
            .on_action(cx.listener(Self::action_right))
            .on_action(cx.listener(Self::action_select_left))
            .on_action(cx.listener(Self::action_select_right))
            .on_action(cx.listener(Self::action_select_all))
            .on_action(cx.listener(Self::action_home))
            .on_action(cx.listener(Self::action_end))
            .on_action(cx.listener(Self::action_paste))
            .on_action(cx.listener(Self::action_cut))
            .on_action(cx.listener(Self::action_copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .size_full()
            .child(TextFieldElement { field: cx.entity() })
    }
}

// ---------------------------------------------------------------------------
// TextFieldElement — custom element that paints selection + cursor via GPU
// ---------------------------------------------------------------------------

pub struct TextFieldElement {
    field: Entity<TextField>,
}

impl IntoElement for TextFieldElement {
    type Element = Self;
    fn into_element(self) -> Self { self }
}

pub struct PrepaintState {
    line: Option<ShapedLine>,
    placeholder_line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl Element for TextFieldElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> { None }

    fn request_layout(
        &mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>,
        window: &mut Window, cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App,
    ) -> PrepaintState {
        let field = self.field.read(cx);
        let content = field.content.clone();
        let placeholder = field.placeholder.clone();
        let selected_range = field.selected_range.clone();
        let cursor_off = field.cursor_offset();
        let ts = window.text_style();
        let font_size = ts.font_size.to_pixels(window.rem_size());

        let is_empty = content.is_empty();
        let run = TextRun {
            len: content.len(),
            font: ts.font(),
            color: hsla(0.0, 0.0, 0.9, 1.0),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window.text_system().shape_line(content, font_size, &[run], None);

        let placeholder_line = if is_empty && !placeholder.is_empty() {
            let placeholder_run = TextRun {
                len: placeholder.len(),
                font: ts.font(),
                color: hsla(0.0, 0.0, 0.45, 1.0),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            Some(window.text_system().shape_line(placeholder, font_size, &[placeholder_run], None))
        } else {
            None
        };

        let (selection, cursor) = if selected_range.is_empty() {
            let x = line.x_for_index(cursor_off);
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + x, bounds.top()),
                        size(px(1.5), bounds.bottom() - bounds.top()),
                    ),
                    rgb(0x4a90d9),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(bounds.left() + line.x_for_index(selected_range.start), bounds.top()),
                        point(bounds.left() + line.x_for_index(selected_range.end), bounds.bottom()),
                    ),
                    hsla(0.60, 0.60, 0.50, 0.35),
                )),
                None,
            )
        };

        PrepaintState { line: Some(line), placeholder_line, cursor, selection }
    }

    fn paint(
        &mut self, _: Option<&GlobalElementId>, _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>, _: &mut (), prepaint: &mut PrepaintState,
        window: &mut Window, cx: &mut App,
    ) {
        let focus = self.field.read(cx).focus_handle.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.field.clone()), cx);

        if let Some(sel) = prepaint.selection.take() { window.paint_quad(sel); }

        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), gpui::TextAlign::Left, None, window, cx).ok();

        if let Some(placeholder_line) = prepaint.placeholder_line.take() {
            placeholder_line.paint(bounds.origin, window.line_height(), gpui::TextAlign::Left, None, window, cx).ok();
        }

        if focus.is_focused(window) {
            if let Some(cur) = prepaint.cursor.take() { window.paint_quad(cur); }
        }

        self.field.update(cx, |f, _| {
            f.last_layout = Some(line);
            f.last_bounds = Some(bounds);
        });
    }
}

// ---------------------------------------------------------------------------
// UTF-16 helpers (needed for EntityInputHandler)
// ---------------------------------------------------------------------------

fn offset_from_utf16(s: &str, utf16: usize) -> usize {
    let mut u8 = 0;
    let mut u16 = 0;
    for ch in s.chars() {
        if u16 >= utf16 { break; }
        u16 += ch.len_utf16();
        u8  += ch.len_utf8();
    }
    u8
}

fn offset_to_utf16(s: &str, utf8: usize) -> usize {
    let mut u8c = 0;
    let mut u16 = 0;
    for ch in s.chars() {
        if u8c >= utf8 { break; }
        u8c += ch.len_utf8();
        u16 += ch.len_utf16();
    }
    u16
}

fn range_from_utf16(s: &str, r: &Range<usize>) -> Range<usize> {
    offset_from_utf16(s, r.start)..offset_from_utf16(s, r.end)
}

fn range_to_utf16(s: &str, r: &Range<usize>) -> Range<usize> {
    offset_to_utf16(s, r.start)..offset_to_utf16(s, r.end)
}
