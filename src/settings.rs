use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentScript {
    pub path: PathBuf,
    pub title: String,
    pub author: String,
    pub scene_count: usize,
    pub last_opened: i64,
    /// Folder-like grouping — a series or franchise this script belongs to.
    #[serde(default)]
    pub group: Option<String>,
    /// A custom cover image, copied into the covers cache directory. When
    /// absent, the Films library paints a generated cover instead.
    #[serde(default)]
    pub cover: Option<PathBuf>,
    /// Bumped each time the generated cover is rerolled from the library's
    /// right-click menu, so a new look is picked without losing the one
    /// before it (0 reproduces the original, stable cover for the path).
    #[serde(default)]
    pub cover_variant: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub vim_enabled: bool,
    /// Show the script as discrete, numbered pages instead of one
    /// continuous scroll.
    #[serde(default)]
    pub paged_mode: bool,
    #[serde(default)]
    pub recent_scripts: Vec<RecentScript>,
    /// Franchise/series names, in the display order chosen on the Films
    /// library screen.
    #[serde(default)]
    pub franchise_order: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { vim_enabled: false, paged_mode: false, recent_scripts: Vec::new(), franchise_order: Vec::new() }
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
/// front of the recent-scripts list (used by the Films library screen). Any
/// franchise the script was already filed under (assigned from the library)
/// is preserved.
pub fn touch_recent(path: &Path, title: &str, author: &str, scene_count: usize) {
    let mut settings = load();
    let existing = settings.recent_scripts.iter().find(|r| r.path == path);
    let existing_group = existing.and_then(|r| r.group.clone());
    let existing_cover = existing.and_then(|r| r.cover.clone());
    let existing_cover_variant = existing.map(|r| r.cover_variant).unwrap_or(0);
    settings.recent_scripts.retain(|r| r.path != path);
    settings.recent_scripts.insert(0, RecentScript {
        path: path.to_path_buf(),
        title: title.to_string(),
        author: author.to_string(),
        scene_count,
        last_opened: chrono::Utc::now().timestamp(),
        group: existing_group,
        cover: existing_cover,
        cover_variant: existing_cover_variant,
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

/// Franchise/series names, in display order (managed from the Films library).
pub fn list_franchises() -> Vec<String> {
    load().franchise_order
}

/// Create a new, empty franchise folder (a no-op if the name is already known).
pub fn create_franchise(name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut settings = load();
    if !settings.franchise_order.iter().any(|f| f == name) {
        settings.franchise_order.push(name.to_string());
        save(&settings);
    }
}

/// Move a franchise one slot earlier (`delta: -1`) or later (`delta: 1`) in
/// the library's display order. A no-op past either end.
pub fn move_franchise(name: &str, delta: i32) {
    let mut settings = load();
    let Some(pos) = settings.franchise_order.iter().position(|f| f == name) else { return; };
    let new_pos = pos as i32 + delta;
    if new_pos < 0 || new_pos as usize >= settings.franchise_order.len() {
        return;
    }
    settings.franchise_order.swap(pos, new_pos as usize);
    save(&settings);
}

/// Assign (`Some(name)`) or clear (`None`) the franchise a script belongs to.
pub fn set_script_group(path: &Path, group: Option<String>) {
    let mut settings = load();
    if let Some(r) = settings.recent_scripts.iter_mut().find(|r| r.path == path) {
        r.group = group;
        save(&settings);
    }
}

/// Set (`Some(path)`, already copied into `covers_dir()`) or clear (`None`)
/// a script's custom cover image.
pub fn set_script_cover(path: &Path, cover: Option<PathBuf>) {
    let mut settings = load();
    if let Some(r) = settings.recent_scripts.iter_mut().find(|r| r.path == path) {
        r.cover = cover;
        save(&settings);
    }
}

/// Directory where custom cover images are cached, created on first use.
pub fn covers_dir() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("rachana").join("covers");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Pick a new look for the generated cover (a no-op when a custom cover
/// image is set — that reroll only affects the fallback generated art).
pub fn reroll_cover(path: &Path) {
    let mut settings = load();
    if let Some(r) = settings.recent_scripts.iter_mut().find(|r| r.path == path) {
        r.cover_variant = r.cover_variant.wrapping_add(1);
        save(&settings);
    }
}

/// Remove a script from the library's recent-scripts shelf. The script's own
/// file on disk is untouched — this only forgets it here.
pub fn forget_script(path: &Path) {
    let mut settings = load();
    settings.recent_scripts.retain(|r| r.path != path);
    save(&settings);
}
