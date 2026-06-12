use std::ops::Range;

use gpui::*;

use crate::model::{Document, FountainElement};
use crate::ui::modal::ModalState;
use crate::ui::text_runs::{element_style, ElementStyle, TextAlign as StyleAlign};

/// Emitted when the user left-clicks inside a block to position the cursor.
pub struct CursorClicked {
    pub char_idx: usize,
}

/// Emitted when the user right-clicks inside a block.
pub struct RightClicked {
    pub position: Point<Pixels>,
    pub char_idx: usize,
}

/// Emitted on every mouse-move while the left button is held down.
pub struct CursorDragged {
    pub char_idx: usize,
}

pub struct BlockView {
    pub block_index: usize,
    pub document: Entity<Document>,
    pub modal: Entity<ModalState>,
    pub focused: bool,
    pub cursor_char: usize,
    pub selection: Option<Range<usize>>,
    /// Window-space Y of this block's top edge, updated each frame via on_children_prepainted.
    pub top_y: f32,
    focus_handle: FocusHandle,
}

impl EventEmitter<CursorClicked> for BlockView {}
impl EventEmitter<RightClicked> for BlockView {}
impl EventEmitter<CursorDragged> for BlockView {}

impl BlockView {
    pub fn new(
        block_index: usize,
        document: Entity<Document>,
        modal: Entity<ModalState>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            block_index,
            document,
            modal,
            focused: false,
            cursor_char: 0,
            selection: None,
            top_y: 0.0,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Return the character index corresponding to the given window-space click position,
    /// accounting for word-wrapped visual lines.
    fn compute_cursor(&self, click_x: Pixels, click_y: Pixels, window: &Window, cx: &mut Context<Self>) -> usize {
        let (text, element) = {
            let doc = self.document.read(cx);
            let Some(block) = doc.blocks.get(self.block_index) else { return 0 };
            (block.text.clone(), block.element.clone())
        };

        let style = element_style(&element);
        let raw: String = {
            let s = if style.uppercase { text.to_uppercase() } else { text };
            s.replace('\n', " ")
        };
        if raw.is_empty() { return 0; }

        let viewport_w = window.viewport_size().width.as_f32();
        let page_left = (viewport_w - 700.0) / 2.0;
        let content_left = page_left + style.left_pad.as_f32();
        let content_width = 700.0 - style.left_pad.as_f32() - style.right_pad.as_f32();
        let wrap_width = px(content_width);

        let run = make_run(&style, raw.len(), None, false, false);
        let shaped = window.text_system().shape_line(
            SharedString::from(raw.clone()),
            style.font_size,
            &[run],
            None,
        );
        let total_width = shaped.width().as_f32();

        let text_indent: f32 = match style.text_align {
            StyleAlign::Left   => 0.0,
            StyleAlign::Right  => (content_width - total_width).max(0.0),
            StyleAlign::Center => ((content_width - total_width) / 2.0).max(0.0),
        };

        // Which visual (wrapped) line was clicked?
        let local_y = (click_y.as_f32() - self.top_y).max(0.0);
        let line_h = style.line_height.as_f32();
        let visual_line = (local_y / line_h).floor() as usize;

        let wrap_starts = find_wrap_starts(&raw, &shaped, wrap_width);
        let num_lines = wrap_starts.len() + 1;
        let visual_line = visual_line.min(num_lines.saturating_sub(1));

        // Byte range for the clicked visual line.
        let line_start_byte: usize = if visual_line == 0 {
            0
        } else {
            wrap_starts[visual_line - 1]
        };
        let line_end_byte: usize = wrap_starts.get(visual_line).copied().unwrap_or(raw.len());

        // Absolute X offset (in the shaped text) at the start of this visual line.
        let line_start_abs_x: f32 = if line_start_byte == 0 {
            0.0
        } else {
            shaped.split_at(line_start_byte).0.width().as_f32()
        };

        // For wrapped lines beyond the first, the indent resets to 0.
        let effective_indent = if visual_line == 0 { text_indent } else { 0.0 };
        let click_in_content = (click_x.as_f32() - content_left - effective_indent).max(0.0);
        let target_abs_x = click_in_content + line_start_abs_x;

        // Find the character whose midpoint is nearest to target_abs_x.
        let chars: Vec<char> = raw.chars().collect();
        let char_start = raw[..line_start_byte].chars().count();
        let mut cur_byte = line_start_byte;
        let mut x_prev = line_start_abs_x;

        for ci in char_start..chars.len() {
            cur_byte += chars[ci].len_utf8();
            let x_next = shaped.split_at(cur_byte).0.width().as_f32();

            if target_abs_x <= (x_prev + x_next) / 2.0 {
                return ci;
            }
            x_prev = x_next;

            if cur_byte >= line_end_byte {
                return ci + 1;
            }
        }
        chars.len()
    }
}

impl Render for BlockView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let read = {
            let doc = self.document.read(cx);
            doc.blocks
                .get(self.block_index)
                .map(|b| (b.element.clone(), b.text.clone()))
        };
        let Some((element, text)) = read else {
            return div().w_full().min_h(px(22.0));
        };

        // Gap suppression: no bottom margin within a speech unit
        let next_element = {
            let doc = self.document.read(cx);
            doc.blocks.get(self.block_index + 1).map(|b| b.element.clone())
        };
        let suppress_gap = matches!(
            (&element, &next_element),
            (
                FountainElement::Character
                | FountainElement::Dialogue
                | FountainElement::Parenthetical,
                Some(
                    FountainElement::Character
                    | FountainElement::Dialogue
                    | FountainElement::Parenthetical
                )
            )
        );

        let style = element_style(&element);
        let mb = if suppress_gap { px(0.0) } else { px(8.0) };

        // Common container style shared by both focused and unfocused paths.
        let container = div()
            .w_full()
            .min_h(style.line_height)
            .mb(mb)
            .pl(style.left_pad)
            .pr(style.right_pad)
            .text_size(style.font_size)
            .line_height(style.line_height)
            .whitespace_normal()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                this.focus_handle.clone().focus(window, cx);
                let char_idx = this.compute_cursor(event.position.x, event.position.y, window, cx);
                this.cursor_char = char_idx;
                this.selection = None;
                cx.emit(CursorClicked { char_idx });
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                this.focus_handle.clone().focus(window, cx);
                let char_idx = this.compute_cursor(event.position.x, event.position.y, window, cx);
                this.cursor_char = char_idx;
                cx.emit(CursorClicked { char_idx });
                cx.emit(RightClicked { position: event.position, char_idx });
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    let cur = this.compute_cursor(event.position.x, event.position.y, window, cx);
                    this.cursor_char = cur;
                    cx.emit(CursorDragged { char_idx: cur });
                }
            }));

        let container = match style.text_align {
            StyleAlign::Right => container.text_right(),
            StyleAlign::Center => container.text_center(),
            StyleAlign::Left => container,
        };

        if self.focused {
            let raw = if style.uppercase { text.to_uppercase() } else { text.clone() };
            let char_count = raw.chars().count();
            let clamp = self.cursor_char.min(char_count);

            // Compute cursor pixel position, accounting for word-wrap at the
            // element's content width. Newlines are replaced with spaces so
            // shape_line doesn't panic; char count is preserved.
            let before_measure: String =
                raw.chars().take(clamp).collect::<String>().replace('\n', " ");
            let wrap_width = px(700.0) - style.left_pad - style.right_pad;
            let cursor_pt = cursor_position_wrapped(
                &before_measure, &style, wrap_width, style.line_height, window,
            );

            let cursor_color = Hsla { h: 0.0, s: 0.0, l: 0.95, a: 1.0 };
            let cursor_bar = div()
                .absolute()
                .left(cursor_pt.x)
                .top(cursor_pt.y)
                .w(px(2.0))
                .h(style.line_height)
                .bg(cursor_color);

            // Render text normally (with optional selection bg) on the layer below.
            let content_w = 700.0 - style.left_pad.as_f32() - style.right_pad.as_f32();
            let sel_color = hsla(0.60, 0.50, 0.50, 0.40);
            let text_runs = plain_runs(&raw, &style);

            let mut inner = div()
                .relative()
                .w_full()
                .min_h(style.line_height);

            // Selection highlight behind text.
            if let Some(sel) = &self.selection {
                for (x, y, w, h) in selection_rects(&raw, &style, sel, wrap_width, content_w, window) {
                    inner = inner.child(
                        div()
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .w(px(w))
                            .h(px(h))
                            .rounded(px(3.0))
                            .bg(sel_color),
                    );
                }
            }

            inner = inner
                .child(StyledText::new(SharedString::from(raw)).with_runs(text_runs))
                .child(cursor_bar);

            container.child(inner)
        } else {
            let raw = if style.uppercase { text.to_uppercase() } else { text.clone() };

            if let Some(sel) = &self.selection {
                let wrap_width = px(700.0) - style.left_pad - style.right_pad;
                let content_w = 700.0 - style.left_pad.as_f32() - style.right_pad.as_f32();
                let sel_color = hsla(0.60, 0.50, 0.50, 0.40);
                let text_runs = plain_runs(&raw, &style);

                let mut wrapper = div().relative().w_full().min_h(style.line_height);
                for (x, y, w, h) in selection_rects(&raw, &style, sel, wrap_width, content_w, window) {
                    wrapper = wrapper.child(
                        div()
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .w(px(w))
                            .h(px(h))
                            .rounded(px(3.0))
                            .bg(sel_color),
                    );
                }
                wrapper = wrapper.child(StyledText::new(SharedString::from(raw)).with_runs(text_runs));
                container.child(wrapper)
            } else {
                let (display, runs) = render_inline_markup(&raw, &style);
                container.child(StyledText::new(SharedString::from(display)).with_runs(runs))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Run helpers
// ---------------------------------------------------------------------------

fn make_run(style: &ElementStyle, len: usize, bg: Option<Hsla>, bold: bool, italic: bool) -> TextRun {
    TextRun {
        len,
        font: Font {
            family: style.font_family.clone(),
            weight: if bold { FontWeight::BOLD } else { style.font_weight },
            style: if italic { FontStyle::Italic } else { style.font_style },
            features: FontFeatures::default(),
            fallbacks: None,
        },
        color: style.color,
        background_color: bg,
        underline: None,
        strikethrough: None,
    }
}

fn plain_runs(text: &str, style: &ElementStyle) -> Vec<TextRun> {
    if text.is_empty() {
        vec![make_run(style, 0, None, false, false)]
    } else {
        vec![make_run(style, text.len(), None, false, false)]
    }
}

/// Compute the pixel position of the cursor (top-left of the cursor bar) within
/// a text block, accounting for word-wrap at `wrap_width`.
///
/// `before` is the text to the left of the cursor with newlines already replaced
/// by spaces (shape_line panics on newlines).
fn cursor_position_wrapped(
    before: &str,
    style: &ElementStyle,
    wrap_width: Pixels,
    line_height: Pixels,
    window: &Window,
) -> Point<Pixels> {
    if before.is_empty() {
        return point(px(0.0), px(0.0));
    }

    let run = make_run(style, before.len(), None, false, false);
    let shaped = window.text_system().shape_line(
        SharedString::from(before.to_string()),
        style.font_size,
        &[run],
        None,
    );

    // Find where each visual line starts (byte offset into `before`).
    let wrap_starts = find_wrap_starts(before, &shaped, wrap_width);

    let visual_line = wrap_starts.len();
    let last_start_byte = wrap_starts.last().copied().unwrap_or(0);
    let last_start_x = if last_start_byte == 0 {
        px(0.0)
    } else {
        shaped.split_at(last_start_byte).0.width()
    };

    let cursor_x = shaped.width() - last_start_x;
    point(cursor_x, line_height * visual_line as f32)
}

/// Return the byte offsets in `text` where each new visual line starts,
/// using the same greedy word-wrap logic as GPUI's text renderer.
fn find_wrap_starts(text: &str, shaped: &ShapedLine, wrap_width: Pixels) -> Vec<usize> {
    let mut wrap_starts: Vec<usize> = Vec::new();
    let mut line_start_byte = 0usize;
    let mut line_start_x = px(0.0);
    // (byte_offset, x_offset) of the most recent word-start candidate
    let mut last_word_start: Option<(usize, Pixels)> = None;
    let mut prev_ch = '\0';

    for (byte_offset, ch) in text.char_indices() {
        let next_byte = byte_offset + ch.len_utf8();

        // Track word-start positions: transition from whitespace to non-whitespace.
        if prev_ch == ' ' && ch != ' ' {
            let x = shaped.split_at(byte_offset).0.width();
            last_word_start = Some((byte_offset, x));
        }

        let char_end_x = shaped.split_at(next_byte).0.width();

        if char_end_x - line_start_x > wrap_width && byte_offset > line_start_byte {
            let (new_byte, new_x) = last_word_start.unwrap_or_else(|| {
                (byte_offset, shaped.split_at(byte_offset).0.width())
            });
            line_start_byte = new_byte;
            line_start_x = new_x;
            wrap_starts.push(new_byte);
            last_word_start = None;
        }

        prev_ch = ch;
    }

    wrap_starts
}

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

const SEL_PAD_LEFT: f32 = 2.0;

/// Returns (x, y, width, height) rectangles in block-local coordinates for each
/// visual-line fragment that falls within `selection`.
///
/// Each natural `\n`-delimited line is shaped independently so the rects align
/// with GPUI's rendered line breaks (rather than treating the whole block as one
/// long word-wrapped string).
fn selection_rects(
    raw: &str,
    style: &ElementStyle,
    selection: &Range<usize>,
    wrap_width: Pixels,
    content_width: f32,
    window: &Window,
) -> Vec<(f32, f32, f32, f32)> {
    if raw.is_empty() || selection.start >= selection.end { return vec![]; }
    let s_byte = char_to_byte(raw, selection.start).min(raw.len());
    let e_byte = char_to_byte(raw, selection.end).min(raw.len());
    if s_byte >= e_byte { return vec![]; }

    let line_h = style.line_height.as_f32();
    let mut rects = Vec::new();
    let mut visual_line_idx: usize = 0;
    let mut raw_byte_cursor: usize = 0;
    let mut first_nl = true;

    for natural_line in raw.split('\n') {
        let nl_start = raw_byte_cursor;
        let nl_end   = raw_byte_cursor + natural_line.len();

        let run_len = natural_line.len().max(1);
        let run = make_run(style, run_len, None, false, false);
        let shaped = window.text_system().shape_line(
            SharedString::from(natural_line.to_string()),
            style.font_size,
            &[run],
            None,
        );

        // Alignment indent: only meaningful for the first visual line (single-line
        // blocks like CHARACTER names that use text_center).
        let text_indent: f32 = if first_nl {
            let tw = shaped.width().as_f32();
            match style.text_align {
                StyleAlign::Left   => 0.0,
                StyleAlign::Right  => (content_width - tw).max(0.0),
                StyleAlign::Center => ((content_width - tw) / 2.0).max(0.0),
            }
        } else {
            0.0
        };
        first_nl = false;

        // Word-wrap sub-lines within this natural line.
        let wrap_starts = find_wrap_starts(natural_line, &shaped, wrap_width);
        let mut sub_ranges: Vec<(usize, usize)> = Vec::new();
        let mut prev = 0usize;
        for &ws in &wrap_starts {
            sub_ranges.push((prev, ws));
            prev = ws;
        }
        sub_ranges.push((prev, natural_line.len()));

        for (sub_idx, &(sub_s, sub_e)) in sub_ranges.iter().enumerate() {
            let abs_start = nl_start + sub_s;
            let abs_end   = nl_start + sub_e;

            let sel_start = s_byte.max(abs_start);
            let sel_end   = e_byte.min(abs_end);

            if sel_start < sel_end {
                let sub_abs_x = if sub_s == 0 { 0.0 } else {
                    shaped.split_at(sub_s).0.width().as_f32()
                };
                let x0 = shaped.split_at(sel_start - nl_start).0.width().as_f32() - sub_abs_x;
                let x1 = shaped.split_at(sel_end   - nl_start).0.width().as_f32() - sub_abs_x;
                let indent = if sub_idx == 0 { text_indent } else { 0.0 };
                let y = visual_line_idx as f32 * line_h;

                rects.push((
                    x0 + indent - SEL_PAD_LEFT,
                    y,
                    (x1 - x0).max(0.0) + SEL_PAD_LEFT,
                    line_h,
                ));
            }

            visual_line_idx += 1;
        }

        raw_byte_cursor = nl_end + 1; // +1 to skip the '\n'
    }

    rects
}

fn build_selection_runs(text: &str, style: &ElementStyle, selection: &Option<Range<usize>>) -> Vec<TextRun> {
    let Some(sel) = selection else {
        return vec![make_run(style, text.len(), None, false, false)];
    };
    let s = char_to_byte(text, sel.start).min(text.len());
    let e = char_to_byte(text, sel.end).min(text.len());
    let sel_bg = Hsla { h: 0.60, s: 0.50, l: 0.50, a: 0.40 };
    let mut runs = Vec::new();
    if s > 0 { runs.push(make_run(style, s, None, false, false)); }
    if e > s { runs.push(make_run(style, e - s, Some(sel_bg), false, false)); }
    if e < text.len() { runs.push(make_run(style, text.len() - e, None, false, false)); }
    if runs.is_empty() { runs.push(make_run(style, text.len(), None, false, false)); }
    runs
}

// ---------------------------------------------------------------------------
// Inline markup renderer (unfocused only)
// ---------------------------------------------------------------------------

fn render_inline_markup(text: &str, style: &ElementStyle) -> (String, Vec<TextRun>) {
    if text.is_empty() {
        return (String::new(), vec![make_run(style, 0, None, false, false)]);
    }
    if !text.contains('*') && !text.contains('_') {
        return (text.to_string(), vec![make_run(style, text.len(), None, false, false)]);
    }

    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut display = String::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;

    macro_rules! flush {
        () => {
            if !plain.is_empty() {
                let bytes = plain.len();
                display.push_str(&plain);
                runs.push(make_run(style, bytes, None, false, false));
                plain.clear();
            }
        };
    }

    while i < n {
        // **bold**
        if i + 1 < n && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(close) = find_close(&chars, i + 2, "**") {
                flush!();
                let content: String = chars[i + 2..close].iter().collect();
                display.push_str(&content);
                runs.push(make_run(style, content.len(), None, true, false));
                i = close + 2;
                continue;
            }
        }
        // *italic*
        if chars[i] == '*' {
            if let Some(close) = find_close(&chars, i + 1, "*") {
                flush!();
                let content: String = chars[i + 1..close].iter().collect();
                display.push_str(&content);
                runs.push(make_run(style, content.len(), None, false, true));
                i = close + 1;
                continue;
            }
        }
        // _underline_
        if chars[i] == '_' {
            if let Some(close) = find_close(&chars, i + 1, "_") {
                flush!();
                let content: String = chars[i + 1..close].iter().collect();
                display.push_str(&content);
                let mut run = make_run(style, content.len(), None, false, false);
                run.underline = Some(UnderlineStyle {
                    color: Some(style.color),
                    thickness: px(1.0),
                    wavy: false,
                });
                runs.push(run);
                i = close + 1;
                continue;
            }
        }
        plain.push(chars[i]);
        i += 1;
    }
    flush!();

    if runs.is_empty() {
        runs.push(make_run(style, display.len(), None, false, false));
    }
    (display, runs)
}

fn find_close(chars: &[char], from: usize, marker: &str) -> Option<usize> {
    let mc: Vec<char> = marker.chars().collect();
    let mlen = mc.len();
    let n = chars.len();
    let mut i = from;
    while i + mlen <= n {
        if chars[i..i + mlen] == mc[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}
