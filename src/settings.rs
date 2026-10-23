use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentScript {
    pub path: PathBuf,
    pub title: String,
    pub author: String,
    pub scene_count: usize,
    pub last_opened: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub vim_enabled: bool,
    #[serde(default)]
    pub recent_scripts: Vec<RecentScript>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { vim_enabled: false, recent_scripts: Vec::new() }
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

/// Record that the script at `path` was just opened or saved, moving it to the
/// front of the recent-scripts list (used by the Films library screen).
pub fn touch_recent(path: &Path, title: &str, author: &str, scene_count: usize) {
    let mut settings = load();
    settings.recent_scripts.retain(|r| r.path != path);
    settings.recent_scripts.insert(0, RecentScript {
        path: path.to_path_buf(),
        title: title.to_string(),
        author: author.to_string(),
        scene_count,
        last_opened: chrono::Utc::now().timestamp(),
    });
    settings.recent_scripts.truncate(24);
    save(&settings);
}

/// Recent scripts whose files still exist on disk, most-recently-opened first.
/// Entries whose files have gone missing are pruned as a side effect.
pub fn load_recent() -> Vec<RecentScript> {
    let mut settings = load();
    let before = settings.recent_scripts.len();
    settings.recent_scripts.retain(|r| r.path.exists());
    if settings.recent_scripts.len() != before {
        save(&settings);
    }
    settings.recent_scripts
}
