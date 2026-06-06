use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use super::element::FountainElement;

static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub id: usize,
    pub element: FountainElement,
    pub text: String,
    pub raw: String,
}

impl Block {
    pub fn new(element: FountainElement, text: String, raw: String) -> Self {
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            element,
            text,
            raw,
        }
    }
}
