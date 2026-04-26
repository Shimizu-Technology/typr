use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_language() -> String {
    "auto".to_string()
}

fn default_text_mode() -> String {
    "raw".to_string()
}

fn default_transform_provider() -> String {
    "none".to_string()
}

fn default_transform_base_url() -> String {
    "http://localhost:1234/v1".to_string()
}

fn default_transform_model() -> String {
    "openai/gpt-oss-20b".to_string()
}

#[serde(default)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub microphone: String,
    pub engine: String,
    #[serde(rename = "whisperModel")]
    pub whisper_model: String,
    #[serde(rename = "groqApiKey")]
    pub groq_api_key: String,
    #[serde(rename = "recordingMode")]
    pub recording_mode: String,
    pub hotkey: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(rename = "textMode", default = "default_text_mode")]
    pub text_mode: String,
    #[serde(rename = "transformProvider", default = "default_transform_provider")]
    pub transform_provider: String,
    #[serde(rename = "transformBaseUrl", default = "default_transform_base_url")]
    pub transform_base_url: String,
    #[serde(rename = "transformModel", default = "default_transform_model")]
    pub transform_model: String,
    #[serde(rename = "transformApiKey", default)]
    pub transform_api_key: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            microphone: "default".to_string(),
            engine: "local".to_string(),
            whisper_model: "small".to_string(),
            groq_api_key: String::new(),
            recording_mode: "toggle".to_string(),
            hotkey: "CmdOrCtrl+Shift+Space".to_string(),
            language: default_language(),
            text_mode: default_text_mode(),
            transform_provider: default_transform_provider(),
            transform_base_url: default_transform_base_url(),
            transform_model: default_transform_model(),
            transform_api_key: String::new(),
        }
    }
}

impl Settings {
    pub fn config_path(app_dir: &PathBuf) -> PathBuf {
        app_dir.join("config.json")
    }

    pub fn load(app_dir: &PathBuf) -> Self {
        let path = Self::config_path(app_dir);
        match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, app_dir: &PathBuf) -> Result<(), String> {
        let path = Self::config_path(app_dir);
        fs::create_dir_all(app_dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.microphone, "default");
        assert_eq!(settings.engine, "local");
        assert_eq!(settings.whisper_model, "small");
        assert_eq!(settings.groq_api_key, "");
        assert_eq!(settings.recording_mode, "toggle");
        assert_eq!(settings.hotkey, "CmdOrCtrl+Shift+Space");
        assert_eq!(settings.language, "auto");
        assert_eq!(settings.text_mode, "raw");
        assert_eq!(settings.transform_provider, "none");
        assert_eq!(settings.transform_base_url, "http://localhost:1234/v1");
        assert_eq!(settings.transform_model, "openai/gpt-oss-20b");
        assert_eq!(settings.transform_api_key, "");
    }

    #[test]
    fn test_save_and_load() {
        let dir = temp_dir().join("typr_test_settings");
        let _ = fs::remove_dir_all(&dir);

        let mut settings = Settings::default();
        settings.engine = "cloud".to_string();
        settings.groq_api_key = "test-key-123".to_string();
        settings.language = "es".to_string();
        settings.text_mode = "polished".to_string();
        settings.transform_provider = "lmstudio".to_string();
        settings.transform_model = "qwen/qwen3-14b".to_string();

        settings.save(&dir).unwrap();
        let loaded = Settings::load(&dir);

        assert_eq!(loaded.engine, "cloud");
        assert_eq!(loaded.groq_api_key, "test-key-123");
        assert_eq!(loaded.language, "es");
        assert_eq!(loaded.text_mode, "polished");
        assert_eq!(loaded.transform_provider, "lmstudio");
        assert_eq!(loaded.transform_model, "qwen/qwen3-14b");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let dir = temp_dir().join("typr_test_missing");
        let _ = fs::remove_dir_all(&dir);
        let settings = Settings::load(&dir);
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn test_load_corrupt_json_returns_default() {
        let dir = temp_dir().join("typr_test_corrupt");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), "not json").unwrap();

        let settings = Settings::load(&dir);
        assert_eq!(settings, Settings::default());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_legacy_json_uses_new_defaults() {
        let dir = temp_dir().join("typr_test_legacy");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("config.json"),
            r#"{
  "microphone": "default",
  "engine": "local",
  "whisperModel": "small",
  "groqApiKey": "",
  "recordingMode": "toggle",
  "hotkey": "CmdOrCtrl+Shift+Space"
}"#,
        )
        .unwrap();

        let settings = Settings::load(&dir);
        assert_eq!(settings.language, "auto");
        assert_eq!(settings.text_mode, "raw");
        assert_eq!(settings.transform_provider, "none");

        let _ = fs::remove_dir_all(&dir);
    }
}
