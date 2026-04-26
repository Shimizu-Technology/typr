#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use typr_lib::audio;
use typr_lib::downloader;
use typr_lib::history;
use typr_lib::history::HistoryEntry;
use typr_lib::paste::paste_text;
use typr_lib::recorder::{Recorder, RecordingState};
use typr_lib::secrets;
use typr_lib::settings::Settings;
use typr_lib::transcribe_local;
use typr_lib::transform::apply_text_transform;

struct AppState {
    recorder: Recorder,
    settings: Mutex<Settings>,
    registered_hotkey: Mutex<String>,
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

fn get_app_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.typr.app")
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

    let mut next_settings = state.settings.lock().unwrap().clone();
    let previous_hotkey = next_settings.hotkey.clone();

    if previous_hotkey != hotkey {
        rebind_hotkey(&app, state.inner(), &previous_hotkey, &hotkey)?;
        next_settings.hotkey = hotkey;
    }

    if let Err(err) = next_settings.save(&state.app_dir) {
        if previous_hotkey != next_settings.hotkey {
            rebind_hotkey(&app, state.inner(), &next_settings.hotkey, &previous_hotkey)?;
        }
        return Err(err);
    }

    *state.settings.lock().unwrap() = next_settings.clone();

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
fn set_provider_secret(kind: String, value: String) -> Result<(), String> {
    let kind = secrets::parse_secret_kind(&kind)?;
    secrets::set_secret(kind, &value)
}

#[tauri::command]
fn clear_provider_secret(kind: String) -> Result<(), String> {
    let kind = secrets::parse_secret_kind(&kind)?;
    secrets::delete_secret(kind)
}

#[tauri::command]
fn list_history_entries(state: State<AppState>) -> Result<Vec<HistoryEntry>, String> {
    history::list_history(&state.app_dir)
}

#[tauri::command]
fn paste_history_entry(state: State<AppState>, id: String) -> Result<(), String> {
    let entry = history::get_history_entry(&state.app_dir, &id)?;
    paste_text(&entry.final_text)
}

#[tauri::command]
async fn reprocess_history_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<HistoryEntry, String> {
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

    if !next_text.is_empty() {
        paste_text(&next_text)?;
    }

    Ok(entry)
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
        "cloud" | "groq" => {
            if !secret_status.has_groq_api_key {
                issues.push(ReadinessIssue {
                    level: "error".to_string(),
                    title: "Groq API key is missing".to_string(),
                    detail:
                        "Store a Groq API key in secure storage before using cloud transcription."
                            .to_string(),
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
            "lmstudio" | "openai-compatible" => {
                if settings.transform_base_url.trim().is_empty() {
                    issues.push(ReadinessIssue {
                        level: "error".to_string(),
                        title: "Rewrite provider base URL is missing".to_string(),
                        detail: "Set a valid LM Studio or OpenAI-compatible base URL.".to_string(),
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

                if settings.transform_provider == "openai-compatible"
                    && !secret_status.has_transform_api_key
                {
                    issues.push(ReadinessIssue {
                        level: "warning".to_string(),
                        title: "No rewrite API key stored".to_string(),
                        detail: "This is fine for local servers that do not require auth, but most remote OpenAI-compatible APIs will need a key.".to_string(),
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
    do_toggle_recording(&app, &state).await
}

/// Shared logic for toggle recording, used by both the Tauri command and hotkey handler.
async fn do_toggle_recording(app: &tauri::AppHandle, state: &AppState) -> Result<String, String> {
    let current_state = state.recorder.get_state();
    match current_state {
        RecordingState::Ready => {
            let mic = state.settings.lock().unwrap().microphone.clone();
            state.recorder.start_recording(app, &mic)?;
            Ok("recording".to_string())
        }
        RecordingState::Recording => {
            let settings = state.settings.lock().unwrap().clone();
            let result = state
                .recorder
                .stop_and_transcribe(app, &settings, &state.app_dir)
                .await?;
            Ok(result)
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
                        Ok(result) => println!("[Typr] Toggle result: {}", result),
                        Err(e) => eprintln!("[Typr] Toggle error: {}", e),
                    },
                    "push-to-talk" => {
                        let current = state.recorder.get_state();
                        if current == RecordingState::Ready {
                            let mic = state.settings.lock().unwrap().microphone.clone();
                            match state.recorder.start_recording(&app, &mic) {
                                Ok(_) => println!("[Typr] Recording started"),
                                Err(e) => eprintln!("[Typr] Start recording error: {}", e),
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
                            Ok(result) => println!("[Typr] Transcription: {}", result),
                            Err(e) => eprintln!("[Typr] Transcription error: {}", e),
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
            app_dir,
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            set_hotkey,
            list_microphones,
            get_recording_state,
            get_secret_status,
            set_provider_secret,
            clear_provider_secret,
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
