#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use typr_lib::audio;
use typr_lib::downloader;
use typr_lib::history;
use typr_lib::history::HistoryEntry;
use typr_lib::logger;
use typr_lib::paste::{check_paste_permission_status, paste_text, TextDeliveryResult};
use typr_lib::recorder::{Recorder, RecordingState, TranscriptionResult};
use typr_lib::secrets;
use typr_lib::settings::Settings;
use typr_lib::transcribe_local;
use typr_lib::transform::apply_text_transform;

struct AppState {
    recorder: Recorder,
    settings: Mutex<Settings>,
    registered_hotkey: Mutex<String>,
    paste_verified_this_session: Mutex<bool>,
    app_dir: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadinessIssue {
    level: String,
    title: String,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadinessStatus {
    can_record: bool,
    summary: String,
    issues: Vec<ReadinessIssue>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupCheck {
    title: String,
    ok: bool,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupStatus {
    platform: String,
    summary: String,
    steps: Vec<String>,
    checks: Vec<SetupCheck>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppFeedback {
    level: String,
    title: String,
    detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryPasteResponse {
    delivery: TextDeliveryResult,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReprocessHistoryResponse {
    entry: HistoryEntry,
    delivery: Option<TextDeliveryResult>,
}

fn get_app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.shimizutechnology.typr")
}

fn emit_feedback(app: &tauri::AppHandle, level: &str, title: &str, detail: &str) {
    let _ = app.emit(
        "app-feedback",
        AppFeedback {
            level: level.to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
        },
    );
}

fn emit_error_feedback(app: &tauri::AppHandle, detail: &str) {
    let (title, normalized_detail) = if detail.contains("Copy automation failed") {
        (
            "Command mode needs Accessibility access",
            "Typr could not read your selected text because macOS blocked copy automation. Enable Accessibility for Typr, or for the debug binary while running from cargo.".to_string(),
        )
    } else if detail.contains("Paste automation failed") {
        (
            "Paste permission failed",
            "Typr captured text, but macOS blocked paste automation. Enable Accessibility for Typr, or for the debug binary while running from cargo.".to_string(),
        )
    } else if detail.contains("selected text") {
        (
            "Command mode needs a selection",
            "Select the text you want to rewrite before triggering Command mode.".to_string(),
        )
    } else if detail.contains("Transform provider error")
        || detail.contains("Transform request failed")
    {
        (
            "Rewrite provider failed",
            "Typr could not reach the rewrite model. Confirm the provider, base URL, model name, and API key if required.".to_string(),
        )
    } else if detail.contains("OpenAI API key not set")
        || detail.contains("OpenAI API key is missing")
    {
        (
            "Cloud transcription is not configured",
            "Store your OpenAI API key securely before using OpenAI transcription.".to_string(),
        )
    } else if detail.contains("OpenAI transcription model is missing") {
        (
            "OpenAI transcription model is missing",
            "Choose a supported OpenAI transcription model before using remote transcription."
                .to_string(),
        )
    } else if detail.contains("Whisper model not found") {
        (
            "Local model is missing",
            "Download the configured Whisper model before using local transcription.".to_string(),
        )
    } else if detail.contains("No audio captured") || detail.contains("No input devices") {
        (
            "Microphone capture failed",
            "Typr could not capture audio. Check your microphone selection and OS microphone permissions.".to_string(),
        )
    } else if detail.contains("Currently transcribing") {
        (
            "Typr is still busy",
            "Wait for the current transcription to finish before starting another one.".to_string(),
        )
    } else {
        ("Typr hit an error", detail.to_string())
    };

    emit_feedback(app, "error", title, normalized_detail.as_str());
}

fn mark_paste_verified(state: &AppState) {
    *state.paste_verified_this_session.lock().unwrap() = true;
}

fn emit_delivery_feedback(
    app: &tauri::AppHandle,
    state: &AppState,
    success_title: &str,
    result: &TranscriptionResult,
) {
    if let Some(delivery) = &result.delivery {
        if delivery.delivered_to_cursor {
            mark_paste_verified(state);
            emit_feedback(app, "success", success_title, &delivery.detail);
        } else {
            emit_feedback(app, "warning", "Copied to clipboard", &delivery.detail);
        }
    } else if !result.final_text.trim().is_empty() {
        emit_feedback(
            app,
            "success",
            success_title,
            "Typr transcribed your audio. There was no text to inject, so nothing was pasted.",
        );
    }
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: Settings) -> Result<(), String> {
    settings.save(&state.app_dir)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn set_hotkey(
    app: tauri::AppHandle,
    state: State<AppState>,
    hotkey: String,
) -> Result<Settings, String> {
    let hotkey = hotkey.trim().to_string();
    if hotkey.is_empty() {
        return Err("Hotkey cannot be empty.".to_string());
    }

    let mut settings_guard = state.settings.lock().unwrap();
    let mut next_settings = settings_guard.clone();
    let previous_hotkey = next_settings.hotkey.clone();

    if previous_hotkey != hotkey {
        rebind_hotkey(&app, state.inner(), &previous_hotkey, &hotkey)?;
        next_settings.hotkey = hotkey;
    }

    if let Err(err) = next_settings.save(&state.app_dir) {
        if previous_hotkey != next_settings.hotkey {
            if let Err(rollback_err) =
                rebind_hotkey(&app, state.inner(), &next_settings.hotkey, &previous_hotkey)
            {
                return Err(format!(
                    "{}. Failed to restore previous hotkey after save failure: {}",
                    err, rollback_err
                ));
            }
        }
        return Err(err);
    }

    *settings_guard = next_settings.clone();

    Ok(next_settings)
}

#[tauri::command]
fn list_microphones() -> Vec<audio::MicDevice> {
    audio::list_microphones()
}

#[tauri::command]
fn get_recording_state(state: State<AppState>) -> RecordingState {
    state.recorder.get_state()
}

#[tauri::command]
fn get_secret_status() -> Result<secrets::SecretStatus, String> {
    secrets::get_secret_status()
}

#[tauri::command]
fn get_debug_log_path() -> String {
    logger::debug_log_path().to_string_lossy().to_string()
}

#[tauri::command]
fn set_provider_secret(kind: String, value: String) -> Result<(), String> {
    let trimmed_value = value.trim();
    if trimmed_value.is_empty() {
        return Err("Secret value must not be empty.".to_string());
    }

    let kind = secrets::parse_secret_kind(&kind)?;
    secrets::set_secret(kind, trimmed_value)
}

#[tauri::command]
fn clear_provider_secret(kind: String) -> Result<(), String> {
    let kind = secrets::parse_secret_kind(&kind)?;
    secrets::delete_secret(kind)
}

#[tauri::command]
fn get_setup_status(state: State<AppState>) -> SetupStatus {
    let audio_status = audio::audio_input_status();
    let paste_status = check_paste_permission_status();
    let paste_verified_this_session = *state.paste_verified_this_session.lock().unwrap();

    let mut checks = vec![SetupCheck {
        title: "Microphone devices".to_string(),
        ok: audio_status.input_device_count > 0,
        detail: if audio_status.input_device_count > 0 {
            format!(
                "{} input device{} found{}.",
                audio_status.input_device_count,
                if audio_status.input_device_count == 1 {
                    ""
                } else {
                    "s"
                },
                if audio_status.has_default_input {
                    " with a default microphone available"
                } else {
                    ", but no default microphone is selected by the OS"
                }
            )
        } else {
            "No input devices were detected. Check your audio hardware or OS privacy settings."
                .to_string()
        },
    }];

    checks.push(SetupCheck {
        title: "Paste automation".to_string(),
        ok: paste_status.ready || paste_verified_this_session,
        detail: if paste_verified_this_session && !paste_status.ready {
            "Typr successfully inserted text into another app during this session. Accessibility detection can be conservative in dev mode.".to_string()
        } else {
            paste_status.detail
        },
    });

    let summary = if checks.iter().all(|check| check.ok) {
        "Setup checks look healthy.".to_string()
    } else {
        "One or more setup checks need attention.".to_string()
    };

    let steps = if cfg!(target_os = "macos") {
        vec![
            "Allow microphone access for Typr when macOS prompts you.".to_string(),
            "Grant Accessibility access so Typr can copy selections and paste text into other apps. In dev mode, add the target/debug/typr binary if needed.".to_string(),
            "Keep LM Studio running if you use Polished or Command mode with a local rewrite model."
                .to_string(),
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            "Confirm your preferred microphone is available in Windows sound settings.".to_string(),
            "If paste automation fails in a target app, run Typr with enough privileges to send keyboard input."
                .to_string(),
            "Keep LM Studio running if you use Polished or Command mode with a local rewrite model."
                .to_string(),
        ]
    } else {
        vec![
            "Confirm your preferred microphone is available before starting Typr.".to_string(),
            "Verify clipboard and keyboard injection permissions for your desktop environment."
                .to_string(),
        ]
    };

    SetupStatus {
        platform: std::env::consts::OS.to_string(),
        summary,
        steps,
        checks,
    }
}

#[tauri::command]
fn list_history_entries(state: State<AppState>) -> Result<Vec<HistoryEntry>, String> {
    history::list_history(&state.app_dir)
}

#[tauri::command]
fn paste_history_entry(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
) -> Result<HistoryPasteResponse, String> {
    let entry = history::get_history_entry(&state.app_dir, &id)?;
    let delivery = paste_text(&app, &entry.final_text, None)?;
    if delivery.delivered_to_cursor {
        mark_paste_verified(state.inner());
    }
    Ok(HistoryPasteResponse { delivery })
}

#[tauri::command]
async fn reprocess_history_entry(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<ReprocessHistoryResponse, String> {
    let settings = state.settings.lock().unwrap().clone();
    let mut entry = history::get_history_entry(&state.app_dir, &id)?;

    if settings.text_mode == "command" && entry.selected_text.is_none() {
        return Err(
            "This transcript has no saved selected text, so it cannot be reprocessed in Command mode."
                .to_string(),
        );
    }

    let next_text =
        apply_text_transform(&settings, &entry.raw_text, entry.selected_text.clone()).await?;
    entry.final_text = next_text.clone();
    entry.engine = settings.engine.clone();
    entry.language = settings.language.clone();
    entry.text_mode = settings.text_mode.clone();
    entry.transform_provider = settings.transform_provider.clone();

    history::update_history_entry(&state.app_dir, &entry)?;

    let delivery = if !next_text.is_empty() {
        let delivery = paste_text(&app, &next_text, None)?;
        if delivery.delivered_to_cursor {
            mark_paste_verified(state.inner());
        }
        Some(delivery)
    } else {
        None
    };

    Ok(ReprocessHistoryResponse { entry, delivery })
}

#[tauri::command]
fn delete_history_entry(state: State<AppState>, id: String) -> Result<(), String> {
    history::delete_history_entry(&state.app_dir, &id)
}

#[tauri::command]
fn clear_history_entries(state: State<AppState>) -> Result<(), String> {
    history::clear_history(&state.app_dir)
}

#[tauri::command]
fn get_readiness_status(state: State<AppState>) -> Result<ReadinessStatus, String> {
    let settings = state.settings.lock().unwrap().clone();
    let registered_hotkey = state.registered_hotkey.lock().unwrap().clone();
    let secret_status = secrets::get_secret_status()?;

    let mut issues = Vec::new();

    if registered_hotkey.is_empty() {
        issues.push(ReadinessIssue {
            level: "error".to_string(),
            title: "Global hotkey is not active".to_string(),
            detail: "Typr could not register its shortcut. Pick a different hotkey if this one is already in use.".to_string(),
        });
    } else if registered_hotkey != settings.hotkey {
        issues.push(ReadinessIssue {
            level: "warning".to_string(),
            title: "Configured hotkey differs from active hotkey".to_string(),
            detail: format!(
                "Settings expect {}, but the active shortcut is {}.",
                settings.hotkey, registered_hotkey
            ),
        });
    }

    match settings.engine.as_str() {
        "local" => {
            if !transcribe_local::is_model_downloaded(&state.app_dir, &settings.whisper_model) {
                issues.push(ReadinessIssue {
                    level: "error".to_string(),
                    title: "Whisper model is missing".to_string(),
                    detail: format!(
                        "Download the {} local model before recording.",
                        settings.whisper_model
                    ),
                });
            }
        }
        "openai" | "cloud" | "groq" => {
            if !secret_status.has_openai_api_key {
                issues.push(ReadinessIssue {
                    level: "error".to_string(),
                    title: "OpenAI API key is missing".to_string(),
                    detail:
                        "Store an OpenAI API key in secure storage before using remote transcription."
                            .to_string(),
                });
            }

            if settings.openai_transcription_model.trim().is_empty() {
                issues.push(ReadinessIssue {
                    level: "error".to_string(),
                    title: "OpenAI transcription model is missing".to_string(),
                    detail: "Select a supported OpenAI transcription model before using remote transcription.".to_string(),
                });
            }
        }
        other => {
            issues.push(ReadinessIssue {
                level: "error".to_string(),
                title: "Unknown transcription engine".to_string(),
                detail: format!("The configured engine \"{}\" is not supported.", other),
            });
        }
    }

    if matches!(settings.text_mode.as_str(), "polished" | "command") {
        match settings.transform_provider.as_str() {
            "none" => issues.push(ReadinessIssue {
                level: "warning".to_string(),
                title: "Rewrite mode has no provider".to_string(),
                detail: format!(
                    "{} mode will fall back to minimally cleaned dictation until you configure a rewrite provider.",
                    if settings.text_mode == "command" {
                        "Command"
                    } else {
                        "Polished"
                    }
                ),
            }),
            "lmstudio" | "openai" | "openai-compatible" => {
                if settings.transform_provider == "lmstudio"
                    && settings.transform_base_url.trim().is_empty()
                {
                    issues.push(ReadinessIssue {
                        level: "error".to_string(),
                        title: "Rewrite provider base URL is missing".to_string(),
                        detail: "Set a valid LM Studio base URL.".to_string(),
                    });
                }

                if settings.transform_model.trim().is_empty() {
                    issues.push(ReadinessIssue {
                        level: "error".to_string(),
                        title: "Rewrite model is missing".to_string(),
                        detail: "Set the model name Typr should use for polished or command mode."
                            .to_string(),
                    });
                }

                if settings.transform_provider == "openai"
                    && !secret_status.has_openai_api_key
                {
                    issues.push(ReadinessIssue {
                        level: "error".to_string(),
                        title: "OpenAI rewrite API key is missing".to_string(),
                        detail: "Store an OpenAI API key before using OpenAI for polished or command mode.".to_string(),
                    });
                }
            }
            other => issues.push(ReadinessIssue {
                level: "error".to_string(),
                title: "Unknown rewrite provider".to_string(),
                detail: format!("The configured provider \"{}\" is not supported.", other),
            }),
        }
    }

    let error_count = issues.iter().filter(|issue| issue.level == "error").count();
    let warning_count = issues
        .iter()
        .filter(|issue| issue.level == "warning")
        .count();
    let can_record = error_count == 0;
    let summary = if issues.is_empty() {
        "Ready to dictate.".to_string()
    } else if error_count > 0 {
        format!(
            "{} blocking issue{} and {} warning{} found.",
            error_count,
            if error_count == 1 { "" } else { "s" },
            warning_count,
            if warning_count == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "Ready, with {} warning{} to review.",
            warning_count,
            if warning_count == 1 { "" } else { "s" }
        )
    };

    Ok(ReadinessStatus {
        can_record,
        summary,
        issues,
    })
}

#[tauri::command]
fn check_model_downloaded(state: State<AppState>, model_size: String) -> bool {
    transcribe_local::is_model_downloaded(&state.app_dir, &model_size)
}

#[tauri::command]
async fn download_model(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    model_size: String,
) -> Result<(), String> {
    let url = transcribe_local::model_download_url(&model_size);
    let model_file = transcribe_local::model_filename(&model_size);
    let dest = state.app_dir.join(&model_file);
    downloader::download_model(app, &url, &dest).await
}

#[tauri::command]
async fn toggle_recording(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    match do_toggle_recording(&app, &state).await {
        Ok(result) => match result {
            ToggleOutcome::RecordingStarted => Ok("recording".to_string()),
            ToggleOutcome::Completed(result) => {
                emit_delivery_feedback(&app, state.inner(), "Dictation complete", &result);
                Ok(result.final_text)
            }
        },
        Err(err) => {
            emit_error_feedback(&app, &err);
            Err(err)
        }
    }
}

/// Shared logic for toggle recording, used by both the Tauri command and hotkey handler.
enum ToggleOutcome {
    RecordingStarted,
    Completed(TranscriptionResult),
}

async fn do_toggle_recording(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<ToggleOutcome, String> {
    let current_state = state.recorder.get_state();
    match current_state {
        RecordingState::Ready => {
            let mic = state.settings.lock().unwrap().microphone.clone();
            state.recorder.start_recording(app, &mic)?;
            Ok(ToggleOutcome::RecordingStarted)
        }
        RecordingState::Recording => {
            let settings = state.settings.lock().unwrap().clone();
            let result = state
                .recorder
                .stop_and_transcribe(app, &settings, &state.app_dir)
                .await?;
            Ok(ToggleOutcome::Completed(result))
        }
        RecordingState::Transcribing => Err("Currently transcribing, please wait".to_string()),
    }
}

fn handle_shortcut_event(app: tauri::AppHandle, shortcut_state: ShortcutState) {
    let state = app.state::<AppState>();
    let mode = state.settings.lock().unwrap().recording_mode.clone();

    match shortcut_state {
        ShortcutState::Pressed => {
            tauri::async_runtime::spawn(async move {
                let state = app.state::<AppState>();
                match mode.as_str() {
                    "toggle" => match do_toggle_recording(&app, state.inner()).await {
                        Ok(result) => {
                            match &result {
                                ToggleOutcome::RecordingStarted => {
                                    println!("[Typr] Toggle result: recording");
                                }
                                ToggleOutcome::Completed(done) => {
                                    println!("[Typr] Toggle result: {}", done.final_text);
                                }
                            }
                            if let ToggleOutcome::Completed(done) = result {
                                emit_delivery_feedback(
                                    &app,
                                    state.inner(),
                                    "Dictation complete",
                                    &done,
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!("[Typr] Toggle error: {}", e);
                            emit_error_feedback(&app, &e);
                        }
                    },
                    "push-to-talk" => {
                        let current = state.recorder.get_state();
                        if current == RecordingState::Ready {
                            let mic = state.settings.lock().unwrap().microphone.clone();
                            match state.recorder.start_recording(&app, &mic) {
                                Ok(_) => println!("[Typr] Recording started"),
                                Err(e) => {
                                    eprintln!("[Typr] Start recording error: {}", e);
                                    emit_error_feedback(&app, &e);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            });
        }
        ShortcutState::Released => {
            if mode == "push-to-talk" {
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    let current = state.recorder.get_state();
                    if current == RecordingState::Recording {
                        let settings = state.settings.lock().unwrap().clone();
                        match state
                            .recorder
                            .stop_and_transcribe(&app, &settings, &state.app_dir)
                            .await
                        {
                            Ok(result) => {
                                println!("[Typr] Transcription: {}", result.final_text);
                                emit_delivery_feedback(
                                    &app,
                                    state.inner(),
                                    "Push-to-talk complete",
                                    &result,
                                );
                            }
                            Err(e) => {
                                eprintln!("[Typr] Transcription error: {}", e);
                                emit_error_feedback(&app, &e);
                            }
                        }
                    }
                });
            }
        }
    }
}

fn register_hotkey_listener(app: &tauri::AppHandle, hotkey: &str) -> Result<(), String> {
    let hotkey = hotkey.trim().to_string();
    let handle = app.clone();

    app.global_shortcut()
        .on_shortcut(hotkey.as_str(), move |_app, shortcut, event| {
            println!(
                "[Typr] Hotkey event: {:?} state={:?}",
                shortcut, event.state
            );
            handle_shortcut_event(handle.clone(), event.state);
        })
        .map_err(|e| format!("Failed to register global shortcut {}: {}", hotkey, e))
}

fn rebind_hotkey(
    app: &tauri::AppHandle,
    state: &AppState,
    old_hotkey: &str,
    new_hotkey: &str,
) -> Result<(), String> {
    let old_hotkey = old_hotkey.trim();
    let new_hotkey = new_hotkey.trim();

    if old_hotkey == new_hotkey {
        return Ok(());
    }

    if !old_hotkey.is_empty() && app.global_shortcut().is_registered(old_hotkey) {
        app.global_shortcut()
            .unregister(old_hotkey)
            .map_err(|e| format!("Failed to unregister old hotkey {}: {}", old_hotkey, e))?;
    }

    match register_hotkey_listener(app, new_hotkey) {
        Ok(()) => {
            *state.registered_hotkey.lock().unwrap() = new_hotkey.to_string();
            Ok(())
        }
        Err(err) => {
            if !old_hotkey.is_empty() {
                match register_hotkey_listener(app, old_hotkey) {
                    Ok(()) => {
                        *state.registered_hotkey.lock().unwrap() = old_hotkey.to_string();
                    }
                    Err(rollback_err) => {
                        *state.registered_hotkey.lock().unwrap() = String::new();
                        return Err(format!(
                            "{}. Failed to restore previous hotkey: {}",
                            err, rollback_err
                        ));
                    }
                }
            } else {
                *state.registered_hotkey.lock().unwrap() = String::new();
            }

            Err(err)
        }
    }
}

fn main() {
    let app_dir = get_app_dir();
    let settings = Settings::load(&app_dir);
    let initial_hotkey = settings.hotkey.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            recorder: Recorder::new(),
            settings: Mutex::new(settings),
            registered_hotkey: Mutex::new(String::new()),
            paste_verified_this_session: Mutex::new(false),
            app_dir,
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            set_hotkey,
            list_microphones,
            get_recording_state,
            get_secret_status,
            get_debug_log_path,
            set_provider_secret,
            clear_provider_secret,
            get_setup_status,
            list_history_entries,
            paste_history_entry,
            reprocess_history_entry,
            delete_history_entry,
            clear_history_entries,
            get_readiness_status,
            check_model_downloaded,
            download_model,
            toggle_recording,
        ])
        .setup(move |app| {
            // Create the overlay window (small mic icon, top-right, always on top)
            let monitor = app.primary_monitor().ok().flatten();
            let (x, y) = if let Some(m) = monitor {
                let size = m.size();
                let scale = m.scale_factor();
                let logical_w = size.width as f64 / scale;
                ((logical_w - 60.0) as i32, 10_i32)
            } else {
                (1380, 10)
            };

            let overlay = WebviewWindowBuilder::new(
                app,
                "overlay",
                WebviewUrl::App("src/overlay.html".into()),
            )
            .title("")
            .inner_size(50.0, 50.0)
            .position(x as f64, y as f64)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .shadow(false)
            .build();

            match overlay {
                Ok(_) => println!("[Typr] Overlay window created"),
                Err(e) => eprintln!("[Typr] Failed to create overlay: {}", e),
            }

            println!("[Typr] Registering global shortcut: {}", initial_hotkey);

            let app_handle = app.handle().clone();
            match register_hotkey_listener(&app_handle, &initial_hotkey) {
                Ok(_) => {
                    *app_handle
                        .state::<AppState>()
                        .registered_hotkey
                        .lock()
                        .unwrap() = initial_hotkey.clone();
                    println!("[Typr] Global shortcut registered successfully");
                }
                Err(e) => eprintln!("[Typr] ERROR: {}", e),
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
