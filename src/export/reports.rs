use std::collections::BTreeMap;

use crate::model::{Document, FountainElement};

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Splits a normalized scene heading (e.g. "INT. OFFICE - DAY") into
/// (location, time_of_day).
fn parse_scene_heading(text: &str) -> (String, String) {
    let stripped = text
        .trim_start_matches("INT./EXT.")
        .trim_start_matches("INT/EXT")
        .trim_start_matches("I/E")
        .trim_start_matches("INT.")
        .trim_start_matches("EXT.")
        .trim();
    match stripped.rsplit_once(" - ") {
        Some((loc, tod)) => (loc.trim().to_string(), tod.trim().to_string()),
        None => (stripped.to_string(), String::new()),
    }
}

pub fn scene_list_csv(doc: &Document) -> String {
    let mut out = String::from("Scene,Heading,Location,Time of Day\n");
    let mut n = 0;
    for b in &doc.blocks {
        if b.element == FountainElement::SceneHeading {
            n += 1;
            let (loc, tod) = parse_scene_heading(&b.text);
            out.push_str(&format!(
                "{},{},{},{}\n",
                n, csv_escape(&b.text), csv_escape(&loc), csv_escape(&tod)
            ));
        }
    }
    out
}

pub fn character_list_csv(doc: &Document) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for b in &doc.blocks {
        if b.element == FountainElement::Character {
            let base = b.text.split(" (").next().unwrap_or(&b.text).trim().to_string();
            *counts.entry(base).or_insert(0) += 1;
        }
    }
    let mut out = String::from("Character,Cue Count\n");
    for (name, count) in counts {
        out.push_str(&format!("{},{}\n", csv_escape(&name), count));
    }
    out
}

pub fn location_list_csv(doc: &Document) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for b in &doc.blocks {
        if b.element == FountainElement::SceneHeading {
            let (loc, _) = parse_scene_heading(&b.text);
            *counts.entry(loc).or_insert(0) += 1;
        }
    }
    let mut out = String::from("Location,Scene Count\n");
    for (loc, count) in counts {
        out.push_str(&format!("{},{}\n", csv_escape(&loc), count));
    }
    out
}

pub fn notes_csv(doc: &Document) -> String {
    let mut out = String::from("Scene,Note\n");
    let mut current_scene = String::from("(before first scene)");
    for b in &doc.blocks {
        match b.element {
            FountainElement::SceneHeading => current_scene = b.text.clone(),
            FountainElement::Note => {
                out.push_str(&format!("{},{}\n", csv_escape(&current_scene), csv_escape(&b.text)));
            }
            _ => {}
        }
    }
    out
}

/// Plain-text dump of just the spoken dialogue (character name once per
/// speech turn, skipping parentheticals), useful for table reads.
pub fn dialogue_txt(doc: &Document) -> String {
    let mut out = String::new();
    let mut current_char: Option<&str> = None;
    let mut name_printed = false;
    for b in &doc.blocks {
        match &b.element {
            FountainElement::Character => {
                current_char = Some(&b.text);
                name_printed = false;
            }
            FountainElement::Dialogue => {
                if let (Some(c), false) = (current_char, name_printed) {
                    out.push_str(c);
                    out.push('\n');
                    name_printed = true;
                }
                out.push_str(&b.text);
                out.push('\n');
            }
            FountainElement::SceneHeading | FountainElement::Transition => {
                if name_printed {
                    out.push('\n');
                }
                current_char = None;
                name_printed = false;
            }
            _ => {}
        }
    }
    out
}
