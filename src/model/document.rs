use std::path::PathBuf;

use super::block::Block;
use super::element::FountainElement;

pub struct Document {
    pub blocks: Vec<Block>,
    pub title: String,
    pub credit: String,
    pub author: String,
    pub source: String,
    pub draft_date: String,
    pub contact: String,
    pub notes: String,
    pub dirty: bool,
    pub path: Option<PathBuf>,
}

impl Document {
    pub fn new() -> Self {
        Self {
            blocks: vec![Block::new(FountainElement::Action, String::new(), String::new())],
            title: String::new(),
            credit: String::new(),
            author: String::new(),
            source: String::new(),
            draft_date: String::new(),
            contact: String::new(),
            notes: String::new(),
            dirty: false,
            path: None,
        }
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn scene_count(&self) -> usize {
        self.blocks
            .iter()
            .filter(|b| b.element == FountainElement::SceneHeading)
            .count()
    }

    /// Set the title-page fields and rebuild the title page block from them.
    /// Shared by the Script Properties panel and the New Script dialog.
    pub fn apply_title_page_fields(
        &mut self,
        title: &str, credit: &str, author: &str, source: &str,
        draft_date: &str, contact: &str, notes: &str,
    ) {
        self.title      = title.to_string();
        self.credit     = credit.to_string();
        self.author     = author.to_string();
        self.source     = source.to_string();
        self.draft_date = draft_date.to_string();
        self.contact    = contact.to_string();
        self.notes      = notes.to_string();

        let mut lines: Vec<String> = Vec::new();
        if !title.is_empty()      { lines.push(format!("Title: {title}")); }
        if !credit.is_empty()     { lines.push(format!("Credit: {credit}")); }
        if !author.is_empty()     { lines.push(format!("Author: {author}")); }
        if !source.is_empty()     { lines.push(format!("Source: {source}")); }
        if !draft_date.is_empty() { lines.push(format!("Draft date: {draft_date}")); }
        if !contact.is_empty()    { lines.push(format!("Contact: {contact}")); }
        if !notes.is_empty() {
            if notes.contains('\n') {
                lines.push("Notes:".to_string());
                for note_line in notes.lines() {
                    lines.push(format!("\t{note_line}"));
                }
            } else {
                lines.push(format!("Notes: {notes}"));
            }
        }
        let raw = lines.join("\n");

        self.blocks.retain(|b| b.element != FountainElement::TitlePage);
        if !raw.is_empty() {
            self.blocks.insert(0, Block::new(FountainElement::TitlePage, raw.clone(), raw));
        }
        self.dirty = true;
    }

    pub fn insert_block(&mut self, after: usize, block: Block) {
        let index = (after + 1).min(self.blocks.len());
        self.blocks.insert(index, block);
    }

    pub fn remove_block(&mut self, index: usize) {
        if index < self.blocks.len() {
            self.blocks.remove(index);
        }
    }

    pub fn replace_text(&mut self, index: usize, text: String) {
        if let Some(block) = self.blocks.get_mut(index) {
            block.text = text;
            self.dirty = true;
        }
    }
}
