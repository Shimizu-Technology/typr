pub mod asr;
pub mod audio;
pub mod cleanup;
pub mod context;
pub mod downloader;
pub mod focus;
pub mod history;
pub mod logger;
pub mod macos_input;
pub mod mic_permission;
pub mod paste;
pub mod recorder;
pub mod secrets;
pub mod settings;
pub mod transcribe_local;
pub mod transcribe_openai;
pub mod transform;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logger::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
