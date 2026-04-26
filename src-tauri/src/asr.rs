use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::secrets::{get_nonempty_secret, SecretKind};
use crate::settings::Settings;
use crate::transcribe_local;
use crate::transcribe_openai;

pub enum AsrProvider {
    LocalWhisper {
        model_path: PathBuf,
        language: String,
    },
    OpenAi {
        api_key: String,
        language: String,
        model: String,
    },
}

impl AsrProvider {
    pub async fn transcribe(&self, app: &AppHandle, audio_path: &Path) -> Result<String, String> {
        match self {
            AsrProvider::LocalWhisper {
                model_path,
                language,
            } => transcribe_local::transcribe_local(app, model_path, audio_path, language).await,
            AsrProvider::OpenAi {
                api_key,
                language,
                model,
            } => transcribe_openai::transcribe_openai(api_key, audio_path, language, model).await,
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
        "openai" | "cloud" | "groq" => {
            let api_key = get_nonempty_secret(SecretKind::OpenAiApiKey)?
                .ok_or_else(|| "OpenAI API key is missing".to_string())?;

            Ok(AsrProvider::OpenAi {
                api_key,
                language,
                model: settings.openai_transcription_model.trim().to_string(),
            })
        }
        other => Err(format!("Unknown ASR provider: {}", other)),
    }
}
