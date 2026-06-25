use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub vim_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { vim_enabled: false }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rachana").join("settings.json"))
}

pub fn load() -> Settings {
    let path = match config_path() {
        Some(p) => p,
        None => return Settings::default(),
    };
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return Settings::default(),
    };
    serde_json::from_str(&data).unwrap_or_default()
}

pub fn save(settings: &Settings) {
    let path = match config_path() {
        Some(p) => p,
        None => return,
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(data) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(&path, data);
    }
}
