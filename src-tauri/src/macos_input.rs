#[cfg(target_os = "macos")]
use crate::logger;
#[cfg(target_os = "macos")]
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
#[cfg(target_os = "macos")]
use std::sync::mpsc;
#[cfg(target_os = "macos")]
use tauri::AppHandle;

#[cfg(target_os = "macos")]
const ANSI_C_KEYCODE: u16 = 8;
#[cfg(target_os = "macos")]
const ANSI_V_KEYCODE: u16 = 9;

#[cfg(target_os = "macos")]
pub fn send_command_shortcut(
    app: &AppHandle,
    key: char,
    target_pid: Option<i32>,
) -> Result<(), String> {
    let keycode = shortcut_keycode(key)?;
    let (tx, rx) = mpsc::channel();

    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            release_common_modifiers()?;

            let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
            let target = shortcut_target_description(target_pid);
            logger::info(&format!(
                "[Typr] Dispatching HID macOS Command shortcut sequence for '{}' via {}",
                key, target
            ));

            enigo
                .key(Key::Meta, Direction::Press)
                .map_err(|e| e.to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(12));

            enigo
                .raw(keycode, Direction::Press)
                .map_err(|e| e.to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(12));

            enigo
                .raw(keycode, Direction::Release)
                .map_err(|e| e.to_string())?;
            std::thread::sleep(std::time::Duration::from_millis(12));

            enigo
                .key(Key::Meta, Direction::Release)
                .map_err(|e| e.to_string())?;

            Ok(())
        })();

        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to schedule macOS keyboard shortcut: {}", e))?;

    rx.recv()
        .map_err(|e| format!("Failed to receive macOS keyboard shortcut result: {}", e))?
}

#[cfg(target_os = "macos")]
pub fn release_common_modifiers() -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    for key in [Key::Meta, Key::Shift, Key::Control, Key::Alt] {
        enigo
            .key(key, Direction::Release)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn shortcut_target_description(target_pid: Option<i32>) -> String {
    if let Some(pid) = target_pid.filter(|pid| *pid > 0) {
        format!("frontmost app after restoring pid {}", pid)
    } else {
        "frontmost app".to_string()
    }
}

#[cfg(target_os = "macos")]
fn shortcut_keycode(key: char) -> Result<u16, String> {
    let keycode = match key.to_ascii_lowercase() {
        'c' => ANSI_C_KEYCODE,
        'v' => ANSI_V_KEYCODE,
        other => {
            return Err(format!(
                "Unsupported macOS shortcut key '{}'; expected a known ANSI keycode mapping",
                other
            ))
        }
    };

    Ok(keycode)
}
