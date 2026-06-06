use std::ops::Range;

use gpui::*;

use crate::model::Document;
use crate::ui::modal::ModalState;
use crate::ui::text_runs::{element_style, ElementStyle, TextAlign as StyleAlign};

pub struct BlockView {
    pub block_index: usize,
    pub document: Entity<Document>,
    pub modal: Entity<ModalState>,
    pub focused: bool,
    pub cursor_char: usize,
    pub selection: Option<Range<usize>>,
    focus_handle: FocusHandle,
}

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
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Render for BlockView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let read = {
            let doc = self.document.read(cx);
            doc.blocks
                .get(self.block_index)
                .map(|b| (b.element.clone(), b.text.clone()))
        };

        let Some((element, text)) = read else {
            return div().w_full().min_h(px(22.0));
        };

        // No bottom gap when the next block is part of the same speech unit.
        let next_element = {
            let doc = self.document.read(cx);
            doc.blocks.get(self.block_index + 1).map(|b| b.element.clone())
        };
        let suppress_gap = matches!(
            (&element, &next_element),
            (
                crate::model::FountainElement::Character
                | crate::model::FountainElement::Dialogue
                | crate::model::FountainElement::Parenthetical,
                Some(
                    crate::model::FountainElement::Character
                    | crate::model::FountainElement::Dialogue
                    | crate::model::FountainElement::Parenthetical
                )
            )
        );

        let style = element_style(&element);
        let raw = if style.uppercase { text.to_uppercase() } else { text.clone() };

        // Focused: show raw markup so the writer can edit the markers.
        // Unfocused: render **bold** / *italic* as styled runs, hide the markers.
        let (display, runs) = if self.focused {
            let display: SharedString = if raw.is_empty() { " ".into() } else { raw.into() };
            let runs = build_runs(&display, &style, true, self.cursor_char, &self.selection);
            (display, runs)
        } else {
            let (processed, runs) = render_inline_markup(&raw, &style);
            let display: SharedString = if processed.is_empty() { "".into() } else { processed.into() };
            (display, runs)
        };

        let fh = self.focus_handle.clone();

        let d = div()
            .w_full()
            .min_h(style.line_height)
            .mb(if suppress_gap { px(0.0) } else { px(8.0) })
            .pl(style.left_pad)
            .pr(style.right_pad)
            .text_size(style.font_size)
            .line_height(style.line_height)
            .whitespace_normal()
            .overflow_hidden()
            .track_focus(&self.focus_handle)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                fh.focus(window, cx);
            });

        let d = match style.text_align {
            StyleAlign::Right => d.text_right(),
            StyleAlign::Center => d.text_center(),
            StyleAlign::Left => d,
        };

        d.child(StyledText::new(display).with_runs(runs))
    }
}

// ---------------------------------------------------------------------------
// Inline markup renderer (unfocused blocks only)
// ---------------------------------------------------------------------------

/// Parse **bold** and *italic* markers, return (display_text, runs).
/// Markers are stripped from display_text; formatting is expressed via TextRun font fields.
fn render_inline_markup(text: &str, style: &ElementStyle) -> (String, Vec<TextRun>) {
    if text.is_empty() {
        return (String::new(), vec![make_run(style, 0, None, false, false)]);
    }
    if !text.contains('*') && !text.contains('_') {
        return (text.to_string(), vec![make_run(style, text.len(), None, false, false)]);
    }

    let mut display = String::new();
    let mut runs: Vec<TextRun> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut i = 0;

    // Accumulate a plain segment, flush into a run when we hit markup.
    let mut plain = String::new();

    let flush_plain = |plain: &mut String, display: &mut String, runs: &mut Vec<TextRun>| {
        if !plain.is_empty() {
            let bytes = plain.len();
            display.push_str(plain);
            runs.push(make_run(style, bytes, None, false, false));
            plain.clear();
        }
    };

    while i < n {
        // **bold**
        if i + 1 < n && chars[i] == '*' && chars[i + 1] == '*' {
            if let Some(close) = find_close(&chars, i + 2, "**") {
                flush_plain(&mut plain, &mut display, &mut runs);
                let content: String = chars[i + 2..close].iter().collect();
                display.push_str(&content);
                runs.push(make_run(style, content.len(), None, true, false));
                i = close + 2;
                continue;
            }
        }
        // *italic* (only when not **)
        if chars[i] == '*' {
            if let Some(close) = find_close(&chars, i + 1, "*") {
                flush_plain(&mut plain, &mut display, &mut runs);
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
                flush_plain(&mut plain, &mut display, &mut runs);
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
    flush_plain(&mut plain, &mut display, &mut runs);

    if runs.is_empty() {
        runs.push(make_run(style, display.len(), None, false, false));
    }
    (display, runs)
}

/// Find the closing marker starting at `from`, returns char index of first marker char.
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

// ---------------------------------------------------------------------------
// Run construction (focused / cursor / selection)
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

fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(text.len())
}

fn build_runs(
    text: &str,
    style: &ElementStyle,
    focused: bool,
    cursor_char: usize,
    selection: &Option<Range<usize>>,
) -> Vec<TextRun> {
    if text.is_empty() {
        return vec![make_run(style, 0, None, false, false)];
    }

    if let Some(sel) = selection {
        let s = char_to_byte(text, sel.start).min(text.len());
        let e = char_to_byte(text, sel.end).min(text.len());
        let sel_bg = Hsla { h: 0.60, s: 0.50, l: 0.50, a: 0.40 };
        let mut runs = Vec::new();
        if s > 0 { runs.push(make_run(style, s, None, false, false)); }
        if e > s { runs.push(make_run(style, e - s, Some(sel_bg), false, false)); }
        if e < text.len() { runs.push(make_run(style, text.len() - e, None, false, false)); }
        if runs.is_empty() { runs.push(make_run(style, text.len(), None, false, false)); }
        return runs;
    }

    if focused {
        let cb = char_to_byte(text, cursor_char);
        if cb < text.len() {
            let ch_len = text[cb..].chars().next().map_or(1, |c| c.len_utf8());
            let nb = cb + ch_len;
            let cur_bg = Hsla { h: 0.0, s: 0.0, l: 0.85, a: 0.80 };
            let mut runs = Vec::new();
            if cb > 0 { runs.push(make_run(style, cb, None, false, false)); }
            runs.push(make_run(style, ch_len, Some(cur_bg), false, false));
            if nb < text.len() { runs.push(make_run(style, text.len() - nb, None, false, false)); }
            return runs;
        }
    }

    vec![make_run(style, text.len(), None, false, false)]
}
