use pest::Parser;
use pest_derive::Parser;

use crate::model::{Block, Document, FountainElement};

#[derive(Parser)]
#[grammar = "src/parser/fountain.pest"]
pub struct FountainParser;

/// Normalize source so the grammar always sees a trailing double-newline.
/// Fountain requires blank-line terminators; without this the last
/// dialogue_block (which consumes its own trailing NEWLINE) may not match.
fn normalize(source: &str) -> String {
    if source.ends_with("\n\n") {
        source.to_string()
    } else if source.ends_with('\n') {
        format!("{}\n", source)
    } else {
        format!("{}\n\n", source)
    }
}

pub fn parse(source: &str) -> Document {
    let src = normalize(source);
    let mut title = String::new();
    let mut author = String::new();

    let pairs = match FountainParser::parse(Rule::document, &src) {
        Ok(p) => p,
        Err(_) => {
            return Document {
                blocks: vec![Block::new(
                    FountainElement::Action,
                    source.to_string(),
                    source.to_string(),
                )],
                title,
                author,
                dirty: false,
            };
        }
    };

    let mut blocks: Vec<Block> = Vec::new();

    for doc_pair in pairs {
        if doc_pair.as_rule() != Rule::document {
            continue;
        }
        for pair in doc_pair.into_inner() {
            match pair.as_rule() {
                Rule::title_page => {
                    let raw = pair.as_str().to_string();
                    for line in raw.lines() {
                        if let Some(colon) = line.find(':') {
                            let key = line[..colon].trim().to_lowercase();
                            let val = line[colon + 1..].trim().to_string();
                            match key.as_str() {
                                "title" => title = val,
                                "author" | "authors" => author = val,
                                _ => {}
                            }
                        }
                    }
                    blocks.push(Block::new(FountainElement::TitlePage, raw.clone(), raw));
                }
                Rule::element => {
                    let raw = pair.as_str().to_string();
                    if let Some(child) = pair.into_inner().next() {
                        push_blocks(&mut blocks, child, raw);
                    }
                }
                Rule::EOI => {}
                _ => {}
            }
        }
    }

    if blocks.is_empty() {
        blocks.push(Block::new(FountainElement::Action, String::new(), String::new()));
    }

    Document { blocks, title, author, dirty: false }
}

fn push_blocks(blocks: &mut Vec<Block>, pair: pest::iterators::Pair<Rule>, raw: String) {
    match pair.as_rule() {
        Rule::scene_heading => {
            let text = pair.as_str().trim().to_uppercase();
            blocks.push(Block::new(FountainElement::SceneHeading, text, raw));
        }
        Rule::forced_scene => {
            let text = pair
                .as_str()
                .trim_start_matches('.')
                .trim()
                .to_uppercase();
            blocks.push(Block::new(FountainElement::SceneHeading, text, raw));
        }
        Rule::forced_action => {
            let text = pair.as_str().trim_start_matches('!').to_string();
            blocks.push(Block::new(FountainElement::Action, text, raw));
        }
        Rule::forced_character => {
            let text = pair
                .as_str()
                .trim_start_matches('@')
                .trim()
                .to_uppercase();
            blocks.push(Block::new(FountainElement::Character, text, raw));
        }
        Rule::transition => {
            let text = pair.as_str().trim().to_string();
            blocks.push(Block::new(FountainElement::Transition, text, raw));
        }
        Rule::section => {
            let s = pair.as_str();
            let depth = s.chars().take_while(|&c| c == '#').count().min(6) as u8;
            let text = s.trim_start_matches('#').trim().to_string();
            blocks.push(Block::new(FountainElement::Section(depth), text, raw));
        }
        Rule::note => {
            let s = pair.as_str();
            let text = s
                .trim_start_matches("[[")
                .trim_end_matches("]]")
                .to_string();
            blocks.push(Block::new(FountainElement::Note, text, raw));
        }
        Rule::page_break => {
            blocks.push(Block::new(FountainElement::PageBreak, String::new(), raw));
        }
        Rule::dialogue_block => {
            let mut sub = pair.into_inner();
            if let Some(char_pair) = sub.next() {
                let char_raw = char_pair.as_str().to_string();
                let char_text = char_raw.trim().to_uppercase();
                blocks.push(Block::new(FountainElement::Character, char_text, char_raw));
            }
            for line_pair in sub {
                if line_pair.as_rule() != Rule::dialogue_line {
                    continue;
                }
                if let Some(content) = line_pair.into_inner().next() {
                    let cr = content.as_str().to_string();
                    let el = match content.as_rule() {
                        Rule::parenthetical => FountainElement::Parenthetical,
                        _ => FountainElement::Dialogue,
                    };
                    blocks.push(Block::new(el, cr.clone(), cr));
                }
            }
        }
        Rule::action_block => {
            let text = pair.as_str().to_string();
            blocks.push(Block::new(FountainElement::Action, text.clone(), raw));
        }
        _ => {
            let text = pair.as_str().to_string();
            blocks.push(Block::new(FountainElement::Action, text.clone(), raw));
        }
    }
}

pub fn serialize(doc: &Document) -> String {
    let mut out = String::new();
    for (i, block) in doc.blocks.iter().enumerate() {
        if i > 0 {
            let prev = &doc.blocks[i - 1];
            // Character→Dialogue/Parenthetical and intra-dialogue transitions
            // use a single newline; everything else gets a blank line.
            let sep = match (&prev.element, &block.element) {
                (FountainElement::Character, FountainElement::Dialogue)
                | (FountainElement::Character, FountainElement::Parenthetical)
                | (FountainElement::Dialogue, FountainElement::Parenthetical)
                | (FountainElement::Parenthetical, FountainElement::Dialogue)
                | (FountainElement::Dialogue, FountainElement::Dialogue)
                | (FountainElement::Parenthetical, FountainElement::Parenthetical) => "\n",
                _ => "\n\n",
            };
            out.push_str(sep);
        }
        let text = match &block.element {
            FountainElement::SceneHeading | FountainElement::Character => {
                block.text.to_uppercase()
            }
            _ => block.raw.clone(),
        };
        out.push_str(&text);
    }
    out
}

/// Classify a single isolated paragraph of text without full-document context.
/// Called on every keystroke from the editor; must stay fast.
pub fn parse_block(source: &str) -> (FountainElement, String) {
    let s = source.trim();

    // Forced prefixes
    if let Some(rest) = s.strip_prefix('!') {
        return (FountainElement::Action, rest.trim().to_string());
    }
    if let Some(rest) = s.strip_prefix('@') {
        return (FountainElement::Character, rest.trim().to_uppercase());
    }
    // ':WORD' — shot / transition marker (non-standard extension)
    if s.starts_with(':') && s.len() > 1 && !s.starts_with("::") {
        return (FountainElement::Transition, s[1..].trim().to_uppercase());
    }
    if s.starts_with('.') && !s.starts_with("..") {
        return (
            FountainElement::SceneHeading,
            s[1..].trim().to_uppercase(),
        );
    }

    // Page break
    if s.starts_with("===") {
        return (FountainElement::PageBreak, String::new());
    }

    // Note
    if s.starts_with("[[") && s.ends_with("]]") {
        return (FountainElement::Note, s[2..s.len() - 2].to_string());
    }

    // Section
    if s.starts_with('#') {
        let depth = s.chars().take_while(|&c| c == '#').count().min(6) as u8;
        let text = s.trim_start_matches('#').trim().to_string();
        return (FountainElement::Section(depth), text);
    }

    // Parenthetical
    if s.starts_with('(') && s.ends_with(')') {
        return (FountainElement::Parenthetical, s.to_string());
    }

    let upper = s.to_uppercase();

    // Scene heading
    if upper.starts_with("INT.")
        || upper.starts_with("EXT.")
        || upper.starts_with("INT/EXT")
        || upper.starts_with("I/E ")
    {
        return (FountainElement::SceneHeading, upper);
    }

    // Transition: all-caps line ending in " TO:" or known phrase
    let all_upper_or_punct = s
        .chars()
        .all(|c| c.is_ascii_uppercase() || c == ' ' || c.is_ascii_punctuation());
    if all_upper_or_punct {
        if s.ends_with(" TO:") || s == "FADE OUT." || s == "FADE TO BLACK." {
            return (FountainElement::Transition, s.to_string());
        }
    }

    // Character: all-caps base (may have lowercase extension in parens)
    let base = s.split(" (").next().unwrap_or(s);
    let is_character = !base.is_empty()
        && base
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == ' ' || c == '\'' || c == '.')
        && base.chars().any(|c| c.is_ascii_uppercase());
    if is_character {
        return (FountainElement::Character, s.to_uppercase());
    }

    (FountainElement::Action, s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_block (fast path, no grammar) ---

    #[test]
    fn scene_heading_int() {
        let (el, text) = parse_block("INT. OFFICE - DAY");
        assert_eq!(el, FountainElement::SceneHeading);
        assert!(text.starts_with("INT."));
    }

    #[test]
    fn scene_heading_ext() {
        let (el, _) = parse_block("EXT. PARK - NIGHT");
        assert_eq!(el, FountainElement::SceneHeading);
    }

    #[test]
    fn scene_heading_forced() {
        let (el, text) = parse_block(".my unlabeled scene");
        assert_eq!(el, FountainElement::SceneHeading);
        assert_eq!(text, "MY UNLABELED SCENE");
    }

    #[test]
    fn character_plain() {
        let (el, _) = parse_block("JOHN");
        assert_eq!(el, FountainElement::Character);
    }

    #[test]
    fn character_vo() {
        let (el, _) = parse_block("JOHN (V.O.)");
        assert_eq!(el, FountainElement::Character);
    }

    #[test]
    fn character_os() {
        let (el, _) = parse_block("MARY (O.S.)");
        assert_eq!(el, FountainElement::Character);
    }

    #[test]
    fn forced_character() {
        let (el, text) = parse_block("@McCLANE");
        assert_eq!(el, FountainElement::Character);
        assert_eq!(text, "MCCLANE");
    }

    #[test]
    fn forced_action() {
        let (el, text) = parse_block("!just action");
        assert_eq!(el, FountainElement::Action);
        assert_eq!(text, "just action");
    }

    #[test]
    fn transition_smash_cut() {
        let (el, _) = parse_block("SMASH CUT TO:");
        assert_eq!(el, FountainElement::Transition);
    }

    #[test]
    fn transition_fade_out() {
        let (el, _) = parse_block("FADE OUT.");
        assert_eq!(el, FountainElement::Transition);
    }

    #[test]
    fn transition_fade_to_black() {
        let (el, _) = parse_block("FADE TO BLACK.");
        assert_eq!(el, FountainElement::Transition);
    }

    #[test]
    fn parenthetical() {
        let (el, text) = parse_block("(quietly)");
        assert_eq!(el, FountainElement::Parenthetical);
        assert_eq!(text, "(quietly)");
    }

    #[test]
    fn section_h1() {
        let (el, text) = parse_block("# ACT ONE");
        assert_eq!(el, FountainElement::Section(1));
        assert_eq!(text, "ACT ONE");
    }

    #[test]
    fn section_h2() {
        let (el, _) = parse_block("## Scene Group");
        assert_eq!(el, FountainElement::Section(2));
    }

    #[test]
    fn note_inline() {
        let (el, text) = parse_block("[[This is a note]]");
        assert_eq!(el, FountainElement::Note);
        assert_eq!(text, "This is a note");
    }

    #[test]
    fn page_break() {
        let (el, _) = parse_block("===");
        assert_eq!(el, FountainElement::PageBreak);
    }

    #[test]
    fn action_fallback() {
        let (el, text) = parse_block("The door creaks open.");
        assert_eq!(el, FountainElement::Action);
        assert_eq!(text, "The door creaks open.");
    }

    // --- full parse ---

    #[test]
    fn title_page() {
        let src = "Title: My Screenplay\nAuthor: Jane Doe\n\nINT. OFFICE - DAY\n\n";
        let doc = parse(src);
        assert_eq!(doc.title, "My Screenplay");
        assert_eq!(doc.author, "Jane Doe");
        assert!(doc.blocks.iter().any(|b| b.element == FountainElement::TitlePage));
    }

    #[test]
    fn dialogue_block_produces_character_and_dialogue() {
        let src = "JOHN\nHello there.\n\n";
        let doc = parse(src);
        let els: Vec<_> = doc.blocks.iter().map(|b| &b.element).collect();
        assert!(
            els.contains(&&FountainElement::Character),
            "expected Character block, got: {:?}", els
        );
        assert!(
            els.contains(&&FountainElement::Dialogue),
            "expected Dialogue block, got: {:?}", els
        );
    }

    #[test]
    fn parenthetical_inside_dialogue() {
        let src = "MARY\n(softly)\nI don't know.\n\n";
        let doc = parse(src);
        let els: Vec<_> = doc.blocks.iter().map(|b| &b.element).collect();
        assert!(els.contains(&&FountainElement::Parenthetical), "{:?}", els);
        assert!(els.contains(&&FountainElement::Dialogue), "{:?}", els);
    }

    #[test]
    fn round_trip_structure() {
        let src = "INT. OFFICE - DAY\n\nThe room is empty.\n\nJOHN\nHello.\n\n";
        let doc = parse(src);
        let serialized = serialize(&doc);
        let doc2 = parse(&serialized);
        assert_eq!(doc.blocks.len(), doc2.blocks.len(), "block count mismatch after round-trip");
        for (a, b) in doc.blocks.iter().zip(doc2.blocks.iter()) {
            assert_eq!(a.element, b.element, "element mismatch after round-trip");
        }
    }
}
