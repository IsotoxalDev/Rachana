use std::path::Path;

use crate::model::Document;
use crate::parser;

pub fn load_fountain(path: &Path) -> Result<Document, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let mut doc = parser::parse(&source);
    doc.path = Some(path.to_path_buf());
    Ok(doc)
}

pub fn save_fountain(doc: &Document, path: &Path) -> Result<(), String> {
    let text = parser::serialize(doc);
    std::fs::write(path, text)
        .map_err(|e| format!("could not write {}: {e}", path.display()))
}
