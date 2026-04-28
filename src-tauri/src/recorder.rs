use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

use crate::asr::build_asr_provider;
use crate::audio::{AudioCaptureSummary, AudioRecorder};
use crate::cleanup::cleanup_text;
use crate::context::capture_selected_text;
use crate::focus::{capture_focus_target, FocusTarget};
use crate::history::append_history_entry;
use crate::history::build_history_entry;
use crate::logger;
use crate::paste::{paste_text, TextDeliveryResult};
use crate::settings::Settings;
use crate::transform::apply_text_transform;

const MIN_TRANSCRIBABLE_DURATION_SECONDS: f32 = 0.30;
const SILENCE_RMS_THRESHOLD: f32 = 0.0005;
const SILENCE_PEAK_THRESHOLD: f32 = 0.005;
const LOW_CONFIDENCE_RMS_THRESHOLD: f32 = 0.005;

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
    focus_target: Arc<Mutex<Option<FocusTarget>>>,
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

        let state_guard = RecordingStateResetGuard::new(self.state.clone(), app.clone());
        let temp_path = app_dir.join("temp_recording.wav");
        let temp_file_guard = TempFileGuard::new(temp_path.clone());

        // Save audio
        let audio_summary = {
            let mut recorder = self.audio_recorder.lock().unwrap();
            recorder.stop_and_save(&temp_path)?
        };
        preserve_last_recording_for_debug(app_dir, &temp_path);
        validate_audio_capture(&audio_summary)?;

        let selected_text = if settings.text_mode == "command" {
            Some(capture_selected_text(app)?)
        } else {
            None
        };

        let asr_provider = build_asr_provider(settings, app_dir)?;
        let raw_text = asr_provider.transcribe(app, &temp_path).await?;

        let cleaned = cleanup_text(&raw_text);
        logger::info(&format!(
            "[Typr] Transcript raw='{}' cleaned='{}'",
            raw_text, cleaned
        ));
        reject_low_confidence_transcript(&cleaned, &audio_summary)?;
        let final_text = apply_text_transform(settings, &cleaned, selected_text.clone()).await?;
        logger::info(&format!("[Typr] Final text='{}'", final_text));

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
            Some(paste_text(app, &final_text, focus_target.as_ref())?)
        } else {
            None
        };

        let result = TranscriptionResult {
            final_text,
            delivery,
        };

        // Keep the guards alive across the async transcription pipeline.
        drop(temp_file_guard);
        drop(state_guard);

        Ok(result)
    }
}

fn validate_audio_capture(summary: &AudioCaptureSummary) -> Result<(), String> {
    if summary.duration_seconds < MIN_TRANSCRIBABLE_DURATION_SECONDS {
        return Err(format!(
            "No clear speech detected. The recording was only {:.2}s long.",
            summary.duration_seconds
        ));
    }

    if summary.rms < SILENCE_RMS_THRESHOLD && summary.peak < SILENCE_PEAK_THRESHOLD {
        return Err(format!(
            "No clear speech detected. The selected microphone '{}' produced near-silent audio (duration {:.2}s, rms {:.6}, peak {:.6}). Check macOS Microphone permission for Typr and confirm the selected input is not muted.",
            summary.device_name, summary.duration_seconds, summary.rms, summary.peak
        ));
    }

    Ok(())
}

fn reject_low_confidence_transcript(
    transcript: &str,
    summary: &AudioCaptureSummary,
) -> Result<(), String> {
    let normalized = transcript
        .trim()
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_ascii_lowercase();

    let known_silence_hallucination = matches!(
        normalized.as_str(),
        "" | "you" | "thank you" | "thanks for watching" | "bye" | "bye bye"
    );

    if is_non_speech_marker(transcript) {
        return Err(format!(
            "No speech transcript detected. Local Whisper returned a non-speech marker ('{}') from audio captured by '{}' (duration {:.2}s, rms {:.6}, peak {:.6}).",
            transcript.trim(),
            summary.device_name,
            summary.duration_seconds,
            summary.rms,
            summary.peak
        ));
    }

    if known_silence_hallucination && summary.rms < LOW_CONFIDENCE_RMS_THRESHOLD {
        return Err(format!(
            "No clear speech detected. Local Whisper returned a likely silence artifact ('{}') from low-level audio captured by '{}' (duration {:.2}s, rms {:.6}, peak {:.6}).",
            transcript.trim(),
            summary.device_name,
            summary.duration_seconds,
            summary.rms,
            summary.peak
        ));
    }

    Ok(())
}

fn is_non_speech_marker(transcript: &str) -> bool {
    let trimmed = transcript.trim();
    if trimmed.len() < 3 {
        return false;
    }

    let bracketed = (trimmed.starts_with('(') && trimmed.ends_with(')'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'));
    if !bracketed {
        return false;
    }

    let inner = trimmed[1..trimmed.len() - 1].trim().to_ascii_lowercase();
    matches!(
        inner.as_str(),
        "applause"
            | "background noise"
            | "beep"
            | "blank audio"
            | "breathing"
            | "buzzing"
            | "clicking"
            | "coughing"
            | "laughter"
            | "machine noise"
            | "machine whirring"
            | "music"
            | "noise"
            | "silence"
            | "static"
            | "whirring"
    )
}

fn preserve_last_recording_for_debug(app_dir: &PathBuf, temp_path: &PathBuf) {
    let debug_path = app_dir.join("last_recording.wav");
    match std::fs::copy(temp_path, &debug_path) {
        Ok(_) => logger::info(&format!(
            "[Typr] Saved last recording debug copy to {}",
            debug_path.display()
        )),
        Err(err) => logger::error(&format!(
            "[Typr] Could not save last recording debug copy: {}",
            err
        )),
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

    #[test]
    fn rejects_parenthetical_non_speech_markers() {
        let summary = AudioCaptureSummary {
            path: PathBuf::from("recording.wav"),
            device_name: "Test Mic".to_string(),
            sample_format: "F32".to_string(),
            duration_seconds: 2.0,
            rms: 0.2,
            peak: 0.8,
            raw_sample_count: 96_000,
            resampled_sample_count: 32_000,
        };

        let error = reject_low_confidence_transcript("(static)", &summary).unwrap_err();
        assert!(error.contains("non-speech marker"));
    }

    #[test]
    fn allows_normal_parenthetical_speech() {
        assert!(!is_non_speech_marker("(testing testing)"));
    }
}
