use std::io::Cursor;

use printpdf::*;

use crate::model::{Document, FountainElement};
use crate::ui::text_runs::{
    COURIER_PRIME_BOLD, COURIER_PRIME_BOLD_ITALIC, COURIER_PRIME_ITALIC, COURIER_PRIME_REGULAR,
};

// Standard US-letter screenplay layout, ported from FountTUI's PDF exporter
// (src/pdf/export/pdf.rs), all measurements in points (72pt = 1in).
const PAGE_WIDTH: f32 = 612.0;
const PAGE_HEIGHT: f32 = 792.0;
const TOP_MARGIN: f32 = 72.0;
const BOTTOM_MARGIN: f32 = 72.0;
const FONT_SIZE: f32 = 12.0;
const FONT_WIDTH: f32 = 7.2; // Courier's fixed 0.6em advance at 12pt

struct Margin {
    left: f32,
    right: f32,
}

fn margins(el: &FountainElement) -> Margin {
    match el {
        FountainElement::Character => Margin { left: 252.0, right: 108.0 },
        FountainElement::Parenthetical => Margin { left: 223.2, right: 180.0 },
        FountainElement::Dialogue | FountainElement::Lyric => Margin { left: 180.0, right: 144.0 },
        FountainElement::Transition => Margin { left: 144.0, right: 144.0 },
        _ => Margin { left: 108.0, right: 72.0 }, // SceneHeading, Action, etc.
    }
}

/// Strips Fountain's inline emphasis markers (`**bold**`, `*italic*`, `_underline_`)
/// down to plain text. Rendering them as distinct font runs is a future enhancement.
fn strip_emphasis(s: &str) -> String {
    s.replace("***", "").replace("**", "").replace('*', "").replace('_', "")
}

/// Greedy word-wrap that preserves existing paragraph breaks (`\n`) and hard-breaks
/// any single word longer than `max_chars`.
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate_len = if line.is_empty() { word.len() } else { line.len() + 1 + word.len() };
            if candidate_len > max_chars && !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
            while line.len() > max_chars {
                let mut idx = max_chars.min(line.len());
                while idx > 0 && !line.is_char_boundary(idx) {
                    idx -= 1;
                }
                out.push(line[..idx].to_string());
                line = line[idx..].to_string();
            }
        }
        out.push(line);
    }
    out
}

fn new_page(pdf: &PdfDocumentReference) -> (PdfPageIndex, PdfLayerIndex) {
    pdf.add_page(Pt(PAGE_WIDTH).into(), Pt(PAGE_HEIGHT).into(), "Layer 1")
}

fn draw_page_number(layer: &PdfLayerReference, num: usize, font: &IndirectFontRef) {
    let text = format!("{num}.");
    let x = PAGE_WIDTH - 72.0 - (text.len() as f32) * FONT_WIDTH;
    let y = PAGE_HEIGHT - TOP_MARGIN + FONT_SIZE;
    layer.use_text(text, FONT_SIZE, Pt(x).into(), Pt(y).into(), font);
}

fn draw_centered(layer: &PdfLayerReference, text: &str, center_x: f32, y: f32, font_size: f32, font: &IndirectFontRef) {
    let width = text.len() as f32 * (font_size * 0.6);
    let x = center_x - width / 2.0;
    layer.use_text(text.to_string(), font_size, Pt(x).into(), Pt(y).into(), font);
}

fn write_title_page(
    pdf: &PdfDocumentReference, page_idx: PdfPageIndex, layer_idx: PdfLayerIndex,
    doc: &Document, regular: &IndirectFontRef, bold: &IndirectFontRef,
) {
    let layer = pdf.get_page(page_idx).get_layer(layer_idx);
    let center_x = PAGE_WIDTH / 2.0;
    let title_y = PAGE_HEIGHT * 0.55;
    const TITLE_SIZE: f32 = 14.0;

    draw_centered(&layer, &doc.title.to_uppercase(), center_x, title_y, TITLE_SIZE, bold);

    let mut y = title_y - TITLE_SIZE * 3.0;
    if !doc.credit.is_empty() {
        draw_centered(&layer, &doc.credit, center_x, y, FONT_SIZE, regular);
        y -= FONT_SIZE * 2.0;
    }
    if !doc.author.is_empty() {
        draw_centered(&layer, &doc.author, center_x, y, FONT_SIZE, regular);
    }

    let mut by = BOTTOM_MARGIN + FONT_SIZE * 3.0;
    if !doc.contact.is_empty() {
        for line in doc.contact.lines().rev() {
            layer.use_text(line.to_string(), FONT_SIZE, Pt(108.0).into(), Pt(by).into(), regular);
            by += FONT_SIZE;
        }
    }
    if !doc.draft_date.is_empty() {
        let x = PAGE_WIDTH - 72.0 - (doc.draft_date.len() as f32) * FONT_WIDTH;
        layer.use_text(doc.draft_date.clone(), FONT_SIZE, Pt(x).into(), Pt(BOTTOM_MARGIN).into(), regular);
    }
    if !doc.source.is_empty() {
        draw_centered(&layer, &doc.source, center_x, title_y - TITLE_SIZE * 6.0, FONT_SIZE, regular);
    }
}

/// Renders a screenplay `Document` to a standard-formatted, paginated PDF (US Letter).
pub fn export_pdf(doc: &Document) -> Vec<u8> {
    let title = if doc.title.is_empty() { "Untitled" } else { &doc.title };
    let (pdf, first_page, first_layer) = PdfDocument::new(title, Pt(PAGE_WIDTH).into(), Pt(PAGE_HEIGHT).into(), "Layer 1");

    let regular = pdf.add_external_font(Cursor::new(COURIER_PRIME_REGULAR)).expect("embed Courier Prime Regular");
    let bold = pdf.add_external_font(Cursor::new(COURIER_PRIME_BOLD)).expect("embed Courier Prime Bold");
    let _italic = pdf.add_external_font(Cursor::new(COURIER_PRIME_ITALIC)).expect("embed Courier Prime Italic");
    let _bold_italic = pdf.add_external_font(Cursor::new(COURIER_PRIME_BOLD_ITALIC)).expect("embed Courier Prime Bold Italic");

    let (mut page_idx, mut layer_idx) = (first_page, first_layer);
    if !doc.title.is_empty() {
        write_title_page(&pdf, page_idx, layer_idx, doc, &regular, &bold);
        let (p, l) = new_page(&pdf);
        page_idx = p;
        layer_idx = l;
    }

    let max_lines_per_page = (((PAGE_HEIGHT - TOP_MARGIN - BOTTOM_MARGIN) / FONT_SIZE) as usize).saturating_sub(1);

    let mut layer = pdf.get_page(page_idx).get_layer(layer_idx);
    let mut line_idx: usize = 0;
    let mut body_page_num: usize = 1;

    for block in &doc.blocks {
        match block.element {
            FountainElement::Section(_) | FountainElement::Note | FountainElement::TitlePage => continue,
            FountainElement::PageBreak => {
                let (p, l) = new_page(&pdf);
                page_idx = p;
                layer_idx = l;
                layer = pdf.get_page(page_idx).get_layer(layer_idx);
                body_page_num += 1;
                draw_page_number(&layer, body_page_num, &regular);
                line_idx = 0;
                continue;
            }
            _ => {}
        }

        let margin = margins(&block.element);
        let max_chars = (((PAGE_WIDTH - margin.left - margin.right) / FONT_WIDTH) as usize).max(1);
        let cleaned = strip_emphasis(&block.text);
        let lines = wrap_text(&cleaned, max_chars);

        let blank_before = line_idx > 0
            && matches!(
                block.element,
                FountainElement::SceneHeading | FountainElement::Character | FountainElement::Transition
            );
        let needed = lines.len() + if blank_before { 1 } else { 0 };

        if line_idx + needed > max_lines_per_page {
            let (p, l) = new_page(&pdf);
            page_idx = p;
            layer_idx = l;
            layer = pdf.get_page(page_idx).get_layer(layer_idx);
            body_page_num += 1;
            draw_page_number(&layer, body_page_num, &regular);
            line_idx = 0;
        } else if blank_before {
            line_idx += 1;
        }

        let font = if block.element == FountainElement::SceneHeading { &bold } else { &regular };
        let right_align = block.element == FountainElement::Transition;

        for line in &lines {
            let y = PAGE_HEIGHT - TOP_MARGIN - (line_idx as f32 + 1.0) * FONT_SIZE;
            let x = if right_align {
                PAGE_WIDTH - margin.right - (line.len() as f32) * FONT_WIDTH
            } else {
                margin.left
            };
            layer.use_text(line.clone(), FONT_SIZE, Pt(x).into(), Pt(y).into(), font);
            line_idx += 1;
        }
    }

    pdf.save_to_bytes().unwrap_or_default()
}
