use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::settings::Settings;

const HISTORY_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub created_at_ms: u64,
    pub engine: String,
    pub language: String,
    pub text_mode: String,
    pub transform_provider: String,
    pub raw_text: String,
    pub final_text: String,
    pub selected_text: Option<String>,
}

pub fn list_history(app_dir: &Path) -> Result<Vec<HistoryEntry>, String> {
    let path = history_path(app_dir);
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse history file: {}", e)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(format!("Failed to read history file: {}", err)),
    }
}

pub fn append_history_entry(app_dir: &Path, entry: HistoryEntry) -> Result<(), String> {
    let mut entries = list_history(app_dir)?;
    entries.insert(0, entry);
    if entries.len() > HISTORY_LIMIT {
        entries.truncate(HISTORY_LIMIT);
    }
    save_history(app_dir, &entries)
}

pub fn update_history_entry(app_dir: &Path, entry: &HistoryEntry) -> Result<(), String> {
    let mut entries = list_history(app_dir)?;
    let Some(index) = entries.iter().position(|item| item.id == entry.id) else {
        return Err("History entry not found.".to_string());
    };

    entries[index] = entry.clone();
    save_history(app_dir, &entries)
}

pub fn get_history_entry(app_dir: &Path, id: &str) -> Result<HistoryEntry, String> {
    let entries = list_history(app_dir)?;
    entries
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or("History entry not found.".to_string())
}

pub fn delete_history_entry(app_dir: &Path, id: &str) -> Result<(), String> {
    let mut entries = list_history(app_dir)?;
    let initial_len = entries.len();
    entries.retain(|entry| entry.id != id);

    if entries.len() == initial_len {
        return Err("History entry not found.".to_string());
    }

    save_history(app_dir, &entries)
}

pub fn clear_history(app_dir: &Path) -> Result<(), String> {
    save_history(app_dir, &Vec::new())
}

pub fn build_history_entry(
    settings: &Settings,
    raw_text: &str,
    final_text: &str,
    selected_text: Option<String>,
) -> HistoryEntry {
    let created_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();

    HistoryEntry {
        id: format!("tx-{}", created_at_ms),
        created_at_ms,
        engine: settings.engine.clone(),
        language: settings.language.clone(),
        text_mode: settings.text_mode.clone(),
        transform_provider: settings.transform_provider.clone(),
        raw_text: raw_text.to_string(),
        final_text: final_text.to_string(),
        selected_text,
    }
}

fn history_path(app_dir: &Path) -> PathBuf {
    app_dir.join("history.json")
}

fn save_history(app_dir: &Path, entries: &[HistoryEntry]) -> Result<(), String> {
    fs::create_dir_all(app_dir).map_err(|e| format!("Failed to create app directory: {}", e))?;
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("Failed to serialize history entries: {}", e))?;
    let path = history_path(app_dir);
    let temp_path = app_dir.join("history.json.tmp");

    fs::write(&temp_path, json).map_err(|e| format!("Failed to write temp history: {}", e))?;

    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(&temp_path, &path)
            .map_err(|e| format!("Failed to replace history file atomically: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        if path.exists() {
            let backup_path = app_dir.join("history.json.bak");
            let _ = fs::remove_file(&backup_path);

            fs::rename(&path, &backup_path)
                .map_err(|e| format!("Failed to prepare history replacement: {}", e))?;

            match fs::rename(&temp_path, &path) {
                Ok(()) => {
                    let _ = fs::remove_file(&backup_path);
                }
                Err(err) => {
                    let _ = fs::rename(&backup_path, &path);
                    let _ = fs::remove_file(&temp_path);
                    return Err(format!("Failed to replace history file: {}", err));
                }
            }
        } else {
            fs::rename(&temp_path, &path)
                .map_err(|e| format!("Failed to move history file into place: {}", e))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn build_history_entry_uses_settings_snapshot() {
        let settings = Settings::default();
        let entry = build_history_entry(&settings, "raw", "final", Some("selection".to_string()));

        assert_eq!(entry.raw_text, "raw");
        assert_eq!(entry.final_text, "final");
        assert_eq!(entry.engine, "local");
        assert_eq!(entry.text_mode, "raw");
        assert_eq!(entry.selected_text.as_deref(), Some("selection"));
    }

    #[test]
    fn save_and_load_history_round_trip() {
        let dir = temp_dir().join("typr_test_history_round_trip");
        let _ = fs::remove_dir_all(&dir);

        let settings = Settings::default();
        let entry = build_history_entry(&settings, "raw", "final", None);
        append_history_entry(&dir, entry.clone()).unwrap();

        let entries = list_history(&dir).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, entry.id);
        assert_eq!(entries[0].final_text, "final");

        let _ = fs::remove_dir_all(&dir);
    }
}
