use std::path::Path;

use crate::model::Document;
use crate::parser::parse;

/// Load a `.fountain` file from disk and return a parsed Document.
/// Returns an error string on failure.
pub fn load_fountain(path: &Path) -> Result<Document, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    Ok(parse(&source))
}
