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
