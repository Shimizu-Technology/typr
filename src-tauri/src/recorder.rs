use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

use crate::asr::build_asr_provider;
use crate::audio::AudioRecorder;
use crate::cleanup::cleanup_text;
use crate::context::capture_selected_text;
use crate::focus::capture_focus_target;
use crate::history::append_history_entry;
use crate::history::build_history_entry;
use crate::paste::{paste_text, TextDeliveryResult};
use crate::settings::Settings;
use crate::transform::apply_text_transform;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RecorderFeedback {
    level: String,
    title: String,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum RecordingState {
    Ready,
    Recording,
    Transcribing,
}

fn update_overlay(app: &AppHandle, state: &RecordingState) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let class = match state {
            RecordingState::Ready => "mic",
            RecordingState::Recording => "mic recording",
            RecordingState::Transcribing => "mic transcribing",
        };
        let js = format!("document.getElementById('mic').className = '{}';", class);
        let _ = overlay.eval(&js);
    }
}

pub struct Recorder {
    state: Arc<Mutex<RecordingState>>,
    audio_recorder: Arc<Mutex<AudioRecorder>>,
    focus_target: Arc<Mutex<Option<String>>>,
}

pub struct TranscriptionResult {
    pub final_text: String,
    pub delivery: Option<TextDeliveryResult>,
}

struct RecordingStateResetGuard {
    state: Arc<Mutex<RecordingState>>,
    app: AppHandle,
}

struct TempFileGuard {
    path: PathBuf,
}

impl RecordingStateResetGuard {
    fn new(state: Arc<Mutex<RecordingState>>, app: AppHandle) -> Self {
        Self { state, app }
    }
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for RecordingStateResetGuard {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap();
        *state = RecordingState::Ready;
        let _ = self.app.emit("recording-state", RecordingState::Ready);
        update_overlay(&self.app, &RecordingState::Ready);
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Ready)),
            audio_recorder: Arc::new(Mutex::new(AudioRecorder::new())),
            focus_target: Arc::new(Mutex::new(None)),
        }
    }

    pub fn get_state(&self) -> RecordingState {
        self.state.lock().unwrap().clone()
    }

    pub fn start_recording(&self, app: &AppHandle, mic_name: &str) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        if *state != RecordingState::Ready {
            return Err("Already recording or transcribing".to_string());
        }

        let mut recorder = self.audio_recorder.lock().unwrap();
        recorder.start(mic_name)?;
        *self.focus_target.lock().unwrap() = capture_focus_target();

        *state = RecordingState::Recording;
        let _ = app.emit("recording-state", RecordingState::Recording);
        update_overlay(app, &RecordingState::Recording);
        Ok(())
    }

    pub async fn stop_and_transcribe(
        &self,
        app: &AppHandle,
        settings: &Settings,
        app_dir: &PathBuf,
    ) -> Result<TranscriptionResult, String> {
        // Stop recording
        {
            let mut state = self.state.lock().unwrap();
            if *state != RecordingState::Recording {
                return Err("Not currently recording".to_string());
            }
            *state = RecordingState::Transcribing;
            let _ = app.emit("recording-state", RecordingState::Transcribing);
            update_overlay(app, &RecordingState::Transcribing);
        }

        let _state_guard = RecordingStateResetGuard::new(self.state.clone(), app.clone());
        let temp_path = app_dir.join("temp_recording.wav");
        let _temp_file_guard = TempFileGuard::new(temp_path.clone());

        // Save audio
        {
            let mut recorder = self.audio_recorder.lock().unwrap();
            recorder.stop_and_save(&temp_path)?;
        }

        let selected_text = if settings.text_mode == "command" {
            Some(capture_selected_text(app)?)
        } else {
            None
        };

        let asr_provider = build_asr_provider(settings, app_dir)?;
        let raw_text = asr_provider.transcribe(app, &temp_path).await?;

        let cleaned = cleanup_text(&raw_text);
        let final_text = apply_text_transform(settings, &cleaned, selected_text.clone()).await?;

        let history_entry =
            build_history_entry(settings, &cleaned, &final_text, selected_text.clone());
        if let Err(err) = append_history_entry(app_dir, history_entry) {
            let _ = app.emit(
                "app-feedback",
                RecorderFeedback {
                    level: "warning".to_string(),
                    title: "Transcript saved, but history update failed".to_string(),
                    detail: format!(
                        "Typr still completed this dictation, but could not update local history: {}",
                        err
                    ),
                },
            );
            eprintln!("[Typr] History append failed: {}", err);
        }

        // Auto-paste
        let focus_target = self.focus_target.lock().unwrap().take();

        let delivery = if !final_text.is_empty() {
            Some(paste_text(app, &final_text, focus_target.as_deref())?)
        } else {
            None
        };

        Ok(TranscriptionResult {
            final_text,
            delivery,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_ready() {
        let recorder = Recorder::new();
        assert_eq!(recorder.get_state(), RecordingState::Ready);
    }
}
