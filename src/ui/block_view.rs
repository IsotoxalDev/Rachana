use std::ops::Range;

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::model::{Document, FountainElement};
use crate::ui::modal::{EditorMode, ModalState};
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

/// Emitted when the user double-clicks a word.
pub struct WordSelected {
    pub start: usize,
    pub end: usize,
}

/// Emitted when the user shift-clicks to extend the selection.
pub struct ShiftClicked {
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
impl EventEmitter<WordSelected> for BlockView {}
impl EventEmitter<ShiftClicked> for BlockView {}

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

    /// Return the character index corresponding to the given window-space click position.
    ///
    /// Handles multi-line blocks (TitlePage, long Action) by shaping each `\n`-delimited
    /// natural line independently — the same approach used by `selection_rects`.
    fn compute_cursor(&self, click_x: Pixels, click_y: Pixels, window: &Window, cx: &mut Context<Self>) -> usize {
        let (text, element) = {
            let doc = self.document.read(cx);
            let Some(block) = doc.blocks.get(self.block_index) else { return 0 };
            (block.text.clone(), block.element.clone())
        };

        let style = element_style(&element);
        let full_text = if style.uppercase { text.to_uppercase() } else { text };
        if full_text.is_empty() { return 0; }

        let viewport_w = window.viewport_size().width.as_f32();
        let page_left = (viewport_w - 700.0) / 2.0;
        let content_left = page_left + style.left_pad.as_f32();
        let content_width = 700.0 - style.left_pad.as_f32() - style.right_pad.as_f32();
        let wrap_width = px(content_width);

        let local_y = (click_y.as_f32() - self.top_y).max(0.0);
        let line_h = style.line_height.as_f32();
        let target_vline = (local_y / line_h).floor() as usize;

        let mut visual_idx: usize = 0;
        let mut char_offset: usize = 0; // chars in full_text before the current natural line

        for natural_line in full_text.split('\n') {
            let nl_chars = natural_line.chars().count();

            let run = make_run(&style, natural_line.len().max(1), None, false, false);
            let shaped = window.text_system().shape_line(
                SharedString::from(natural_line.to_string()),
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

            let wrap_starts = find_wrap_starts(natural_line, &shaped, wrap_width);
            let mut sub_ranges: Vec<(usize, usize)> = Vec::new();
            let mut prev = 0usize;
            for &ws in &wrap_starts {
                sub_ranges.push((prev, ws));
                prev = ws;
            }
            sub_ranges.push((prev, natural_line.len()));

            for (sub_idx, &(sub_s, sub_e)) in sub_ranges.iter().enumerate() {
                if visual_idx == target_vline {
                    let sub_abs_x = if sub_s == 0 { 0.0f32 } else {
                        shaped.split_at(sub_s).0.width().as_f32()
                    };
                    let effective_indent = if sub_idx == 0 { text_indent } else { 0.0 };
                    let click_in_content = (click_x.as_f32() - content_left - effective_indent).max(0.0);
                    let target_abs_x = click_in_content + sub_abs_x;

                    let chars: Vec<char> = natural_line.chars().collect();
                    let sub_char_start = natural_line[..sub_s].chars().count();
                    let mut cur_byte = sub_s;
                    let mut x_prev = sub_abs_x;

                    for ci in sub_char_start..chars.len() {
                        cur_byte += chars[ci].len_utf8();
                        let x_next = shaped.split_at(cur_byte).0.width().as_f32();
                        if target_abs_x <= (x_prev + x_next) / 2.0 {
                            return char_offset + ci;
                        }
                        x_prev = x_next;
                        if cur_byte >= sub_e {
                            return char_offset + ci + 1;
                        }
                    }
                    return char_offset + nl_chars;
                }
                visual_idx += 1;
            }

            char_offset += nl_chars + 1; // +1 for the '\n' character
        }

        full_text.chars().count()
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
        let suppress_gap = element == FountainElement::TitlePage
            || matches!(
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
                if event.click_count >= 3 {
                    let char_count = this.document.read(cx)
                        .blocks.get(this.block_index)
                        .map(|b| b.text.chars().count())
                        .unwrap_or(0);
                    cx.emit(WordSelected { start: 0, end: char_count });
                } else if event.click_count >= 2 {
                    let text = this.document.read(cx)
                        .blocks.get(this.block_index)
                        .map(|b| b.text.clone())
                        .unwrap_or_default();
                    let range = word_range_at(&text, char_idx);
                    cx.emit(WordSelected { start: range.start, end: range.end });
                } else if event.modifiers.shift {
                    cx.emit(ShiftClicked { char_idx });
                } else {
                    this.cursor_char = char_idx;
                    this.selection = None;
                    cx.emit(CursorClicked { char_idx });
                }
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

            // Text before the cursor — keep \n characters so cursor_position_wrapped
            // can place the caret on the correct visual row for multi-line blocks.
            let before_measure: String = raw.chars().take(clamp).collect();
            let wrap_width = px(700.0) - style.left_pad - style.right_pad;
            let cursor_pt = cursor_position_wrapped(
                &before_measure, &style, wrap_width, style.line_height, window,
            );

            let vim_normal = {
                let m = self.modal.read(cx);
                m.enabled && matches!(m.mode, EditorMode::Normal | EditorMode::Visual { .. })
            };

            let cursor_bar = if vim_normal {
                let char_w = raw.chars().nth(clamp)
                    .filter(|ch| *ch != '\n')
                    .map(|ch| {
                        let s = ch.to_string();
                        let run = make_run(&style, s.len(), None, false, false);
                        let shaped = window.text_system().shape_line(
                            SharedString::from(s), style.font_size, &[run], None,
                        );
                        shaped.width().max(px(2.0))
                    })
                    .unwrap_or_else(|| px(style.font_size.as_f32() * 0.6));
                div()
                    .absolute()
                    .left(cursor_pt.x)
                    .top(cursor_pt.y)
                    .w(char_w)
                    .h(style.line_height)
                    .bg(Hsla { h: 0.0, s: 0.0, l: 0.95, a: 1.0 })
            } else {
                div()
                    .absolute()
                    .left(cursor_pt.x)
                    .top(cursor_pt.y)
                    .w(px(2.0))
                    .h(style.line_height)
                    .bg(Hsla { h: 0.0, s: 0.0, l: 0.95, a: 1.0 })
            };

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
                let rects = selection_rects(&raw, &style, sel, wrap_width, content_w, window);
                inner = apply_selection_rects(inner, rects, sel_color);
            }

            // Block cursor goes behind text so the character remains readable;
            // line cursor goes in front (it's only 2px so it doesn't obscure text).
            if vim_normal {
                inner = inner
                    .child(cursor_bar)
                    .child(StyledText::new(SharedString::from(raw)).with_runs(text_runs));
            } else {
                inner = inner
                    .child(StyledText::new(SharedString::from(raw)).with_runs(text_runs))
                    .child(cursor_bar);
            }

            container.child(inner)
        } else {
            let raw = if style.uppercase { text.to_uppercase() } else { text.clone() };

            if let Some(sel) = &self.selection {
                let wrap_width = px(700.0) - style.left_pad - style.right_pad;
                let content_w = 700.0 - style.left_pad.as_f32() - style.right_pad.as_f32();
                let sel_color = hsla(0.60, 0.50, 0.50, 0.40);
                let text_runs = plain_runs(&raw, &style);

                let rects = selection_rects(&raw, &style, sel, wrap_width, content_w, window);
                let mut wrapper = apply_selection_rects(
                    div().relative().w_full().min_h(style.line_height),
                    rects,
                    sel_color,
                );
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
// Visual-line helpers (used by DocumentView for j/k navigation)
// ---------------------------------------------------------------------------

/// Returns the visual-line index and content-local x offset (pixels from the
/// left edge of the wrap segment) for `cursor_char` within `text`.
///
/// Uses the full text so that wrap boundaries are computed from the real layout,
/// not from a truncated prefix (which would appear to fit in fewer lines).
pub(crate) fn visual_line_and_x(
    text: &str,
    cursor_char: usize,
    style: &ElementStyle,
    wrap_width: Pixels,
    window: &Window,
) -> (usize, f32) {
    let mut visual_idx = 0usize;
    let mut char_offset = 0usize;

    for nl in text.split('\n') {
        let nl_chars = nl.chars().count();

        if nl.is_empty() {
            if cursor_char == char_offset {
                return (visual_idx, 0.0);
            }
            visual_idx += 1;
            char_offset += 1;
            continue;
        }

        let run = make_run(style, nl.len(), None, false, false);
        let shaped = window.text_system().shape_line(
            SharedString::from(nl.to_string()), style.font_size, &[run], None,
        );
        let wrap_starts = find_wrap_starts(nl, &shaped, wrap_width);

        let mut sub_starts: Vec<usize> = vec![0];
        sub_starts.extend(&wrap_starts);
        let n_segs = sub_starts.len();

        for seg_idx in 0..n_segs {
            let sub_s = sub_starts[seg_idx];
            let sub_e = if seg_idx + 1 < n_segs { sub_starts[seg_idx + 1] } else { nl.len() };
            let seg_char_start = char_offset + nl[..sub_s].chars().count();
            let seg_char_end   = char_offset + nl[..sub_e].chars().count();
            let is_last_seg    = seg_idx + 1 == n_segs;

            let in_seg = cursor_char >= seg_char_start
                && (cursor_char < seg_char_end || (is_last_seg && cursor_char == seg_char_end));

            if in_seg {
                let chars_in     = cursor_char - seg_char_start;
                let byte_in      = sub_s + nl[sub_s..].chars().take(chars_in).map(|c| c.len_utf8()).sum::<usize>();
                let seg_start_x  = if sub_s == 0 { 0.0 } else { shaped.split_at(sub_s).0.width().as_f32() };
                let cursor_abs_x = shaped.split_at(byte_in).0.width().as_f32();
                return (visual_idx, cursor_abs_x - seg_start_x);
            }

            visual_idx += 1;
        }

        char_offset += nl_chars + 1;
    }

    (visual_idx.saturating_sub(1), 0.0)
}

/// Returns the total number of visual lines in `text` given the wrap width.
pub(crate) fn count_visual_lines(
    text: &str,
    style: &ElementStyle,
    wrap_width: Pixels,
    window: &Window,
) -> usize {
    if text.is_empty() {
        return 1;
    }
    let mut total = 0usize;
    for nl in text.split('\n') {
        if nl.is_empty() {
            total += 1;
            continue;
        }
        let run = make_run(style, nl.len(), None, false, false);
        let shaped = window.text_system().shape_line(
            SharedString::from(nl.to_string()), style.font_size, &[run], None,
        );
        total += find_wrap_starts(nl, &shaped, wrap_width).len() + 1;
    }
    total
}

/// Returns the char index in `text` that best matches `target_x` on `visual_line`.
/// `target_x` is pixels from the left of the wrap segment (content-relative,
/// same coordinate space that `visual_line_and_x` returns).
pub(crate) fn char_at_visual_line_x(
    text: &str,
    style: &ElementStyle,
    visual_line: usize,
    target_x: f32,
    wrap_width: Pixels,
    window: &Window,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut visual_idx = 0usize;
    let mut char_offset = 0usize;

    for nl in text.split('\n') {
        let nl_chars = nl.chars().count();

        if nl.is_empty() {
            if visual_idx == visual_line {
                return char_offset;
            }
            visual_idx += 1;
            char_offset += 1;
            continue;
        }

        let run = make_run(style, nl.len(), None, false, false);
        let shaped = window.text_system().shape_line(
            SharedString::from(nl.to_string()), style.font_size, &[run], None,
        );
        let wrap_starts = find_wrap_starts(nl, &shaped, wrap_width);

        let mut sub_ranges: Vec<(usize, usize)> = Vec::new();
        let mut prev = 0usize;
        for &ws in &wrap_starts {
            sub_ranges.push((prev, ws));
            prev = ws;
        }
        sub_ranges.push((prev, nl.len()));

        for &(sub_s, sub_e) in &sub_ranges {
            if visual_idx == visual_line {
                let sub_abs_x = if sub_s == 0 { 0.0f32 } else {
                    shaped.split_at(sub_s).0.width().as_f32()
                };
                let abs_target = target_x + sub_abs_x;
                let chars: Vec<char> = nl.chars().collect();
                let sub_char_start = nl[..sub_s].chars().count();
                let sub_char_end   = nl[..sub_e].chars().count();
                let mut cur_byte = sub_s;
                let mut x_prev = sub_abs_x;
                for ci in sub_char_start..sub_char_end {
                    cur_byte += chars[ci].len_utf8();
                    let x_next = shaped.split_at(cur_byte).0.width().as_f32();
                    if abs_target <= (x_prev + x_next) / 2.0 {
                        return char_offset + ci;
                    }
                    x_prev = x_next;
                }
                return char_offset + sub_char_end;
            }
            visual_idx += 1;
        }

        char_offset += nl_chars + 1; // +1 for '\n'
    }

    text.chars().count()
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
/// a text block, accounting for word-wrap and real `\n` line breaks.
///
/// Each `\n`-delimited natural line is shaped independently so that multi-line
/// blocks (TitlePage, long Action) place the cursor on the correct visual row.
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

    let natural_lines: Vec<&str> = before.split('\n').collect();
    let mut total_visual: usize = 0;

    // Count visual lines contributed by every natural line except the last.
    for line in &natural_lines[..natural_lines.len().saturating_sub(1)] {
        if line.is_empty() {
            total_visual += 1;
        } else {
            let run = make_run(style, line.len(), None, false, false);
            let shaped = window.text_system().shape_line(
                SharedString::from(line.to_string()),
                style.font_size,
                &[run],
                None,
            );
            total_visual += find_wrap_starts(line, &shaped, wrap_width).len() + 1;
        }
    }

    // Last natural line: compute the cursor x and sub-line index.
    let last = natural_lines.last().copied().unwrap_or("");
    if last.is_empty() {
        return point(px(0.0), line_height * total_visual as f32);
    }

    let run = make_run(style, last.len(), None, false, false);
    let shaped = window.text_system().shape_line(
        SharedString::from(last.to_string()),
        style.font_size,
        &[run],
        None,
    );

    let wrap_starts = find_wrap_starts(last, &shaped, wrap_width);
    let sub_line = wrap_starts.len();
    let last_start_byte = wrap_starts.last().copied().unwrap_or(0);
    let last_start_x = if last_start_byte == 0 {
        px(0.0)
    } else {
        shaped.split_at(last_start_byte).0.width()
    };

    let cursor_x = shaped.width() - last_start_x;
    point(cursor_x, line_height * (total_visual + sub_line) as f32)
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

fn word_range_at(text: &str, char_idx: usize) -> std::ops::Range<usize> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    if n == 0 { return 0..0; }
    let i = char_idx.min(n.saturating_sub(1));
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '-' || c == '_';
    if !is_word(chars[i]) {
        return i..(i + 1).min(n);
    }
    let mut start = i;
    let mut end = i + 1;
    while start > 0 && is_word(chars[start - 1]) { start -= 1; }
    while end < n && is_word(chars[end]) { end += 1; }
    start..end
}

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

const SEL_PAD: f32 = 2.0;

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

            let y = visual_line_idx as f32 * line_h;

            if natural_line.is_empty() && s_byte <= abs_start && abs_start < e_byte {
                // Empty natural line inside selection: draw a full-width rect so the
                // blank line doesn't punch a hole in the highlight.
                rects.push((-SEL_PAD, y, content_width + 2.0 * SEL_PAD, line_h));
            } else if sel_start < sel_end {
                let sub_abs_x = if sub_s == 0 { 0.0 } else {
                    shaped.split_at(sub_s).0.width().as_f32()
                };
                let x0 = shaped.split_at(sel_start - nl_start).0.width().as_f32() - sub_abs_x;
                let x1 = shaped.split_at(sel_end   - nl_start).0.width().as_f32() - sub_abs_x;
                let indent = if sub_idx == 0 { text_indent } else { 0.0 };

                rects.push((
                    x0 + indent - SEL_PAD,
                    y,
                    (x1 - x0).max(0.0) + 2.0 * SEL_PAD,
                    line_h,
                ));
            }

            visual_line_idx += 1;
        }

        raw_byte_cursor = nl_end + 1; // +1 to skip the '\n'
    }

    rects
}

/// Append selection highlight children to `target` with adaptive per-corner rounding.
///
/// Corner rules (same logic for all four corners):
/// - First rect: top corners always rounded.
/// - Last rect: bottom corners always rounded.
/// - Between adjacent rects: a corner is rounded when the current rect extends
///   beyond its neighbour on that side (convex); flat when edges are aligned;
///   flat when the neighbour extends beyond the current (concave, no overlay).
/// - Rects are pixel-snapped so they touch without gaps or alpha-bleed overlap.
fn apply_selection_rects(mut target: Div, rects: Vec<(f32, f32, f32, f32)>, color: Hsla) -> Div {
    let n = rects.len();
    if n == 0 { return target; }

    // Tolerance for "same edge" — ignores minor word-wrap width variations.
    const EPS: f32 = 8.0;
    const R: f32 = 3.0;

    for (i, &(x, y, w, h)) in rects.iter().enumerate() {
        let right   = x + w;
        let is_last = i + 1 == n;

        let prev_right = if i > 0 { Some(rects[i - 1].0 + rects[i - 1].2) } else { None };
        let next_right = if !is_last { Some(rects[i + 1].0 + rects[i + 1].2) } else { None };
        let prev_left  = if i > 0 { Some(rects[i - 1].0) } else { None };
        let next_left  = if !is_last { Some(rects[i + 1].0) } else { None };

        // Round a corner when this rect extends past its neighbour on that side.
        let round_tl = prev_left .map_or(true, |pl| x     < pl - EPS);
        let round_tr = prev_right.map_or(true, |pr| right > pr + EPS);
        let round_bl = next_left .map_or(true, |nl| x     < nl - EPS);
        let round_br = next_right.map_or(true, |nr| right > nr + EPS);

        // Pixel-snap y so adjacent rects touch exactly (no gap, no alpha-bleed overlap).
        let draw_h = (y + h).round() - y.round();

        target = target.child(
            div()
                .absolute()
                .left(px(x)).top(px(y.round()))
                .w(px(w)).h(px(draw_h))
                .bg(color)
                .when(round_tl, |d| d.rounded_tl(px(R)))
                .when(round_tr, |d| d.rounded_tr(px(R)))
                .when(round_bl, |d| d.rounded_bl(px(R)))
                .when(round_br, |d| d.rounded_br(px(R)))
        );
    }

    target
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
