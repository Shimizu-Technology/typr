use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::secrets::{get_secret, SecretKind};
use crate::settings::Settings;
use crate::transcribe_groq;
use crate::transcribe_local;

pub enum AsrProvider {
    LocalWhisper {
        model_path: PathBuf,
        language: String,
    },
    Groq {
        api_key: String,
        language: String,
    },
}

impl AsrProvider {
    pub async fn transcribe(&self, app: &AppHandle, audio_path: &Path) -> Result<String, String> {
        match self {
            AsrProvider::LocalWhisper {
                model_path,
                language,
            } => transcribe_local::transcribe_local(app, model_path, audio_path, language).await,
            AsrProvider::Groq { api_key, language } => {
                transcribe_groq::transcribe_groq(api_key, audio_path, language).await
            }
        }
    }
}

pub fn build_asr_provider(settings: &Settings, app_dir: &Path) -> Result<AsrProvider, String> {
    let language = settings.language.trim().to_string();

    match settings.engine.as_str() {
        "local" => Ok(AsrProvider::LocalWhisper {
            model_path: app_dir.join(transcribe_local::model_filename(&settings.whisper_model)),
            language,
        }),
        "cloud" | "groq" => Ok(AsrProvider::Groq {
            api_key: get_secret(SecretKind::GroqApiKey)?.unwrap_or_default(),
            language,
        }),
        other => Err(format!("Unknown ASR provider: {}", other)),
    }
}
