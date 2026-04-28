#[cfg(target_os = "macos")]
use crate::logger;
#[cfg(target_os = "macos")]
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
#[cfg(target_os = "macos")]
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
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

            let target = shortcut_target_description(target_pid);
            logger::info(&format!(
                "[Typr] Dispatching CoreGraphics macOS Command shortcut sequence for '{}' via {}",
                key, target
            ));

            send_core_graphics_command_shortcut(keycode, target_pid)?;

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
fn send_core_graphics_command_shortcut(
    keycode: u16,
    target_pid: Option<i32>,
) -> Result<(), String> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Could not create CoreGraphics HID event source".to_string())?;
    let command_flags = CGEventFlags::CGEventFlagCommand;

    let key_down = CGEvent::new_keyboard_event(source.clone(), keycode, true)
        .map_err(|_| "Could not create CoreGraphics key-down event".to_string())?;
    key_down.set_flags(command_flags);
    let key_up = CGEvent::new_keyboard_event(source, keycode, false)
        .map_err(|_| "Could not create CoreGraphics key-up event".to_string())?;
    key_up.set_flags(command_flags);

    post_keyboard_event(&key_down, target_pid);
    std::thread::sleep(std::time::Duration::from_millis(18));
    post_keyboard_event(&key_up, target_pid);
    std::thread::sleep(std::time::Duration::from_millis(18));

    Ok(())
}

#[cfg(target_os = "macos")]
fn post_keyboard_event(event: &CGEvent, target_pid: Option<i32>) {
    if let Some(pid) = target_pid.filter(|pid| *pid > 0) {
        event.post_to_pid(pid);
    } else {
        event.post(CGEventTapLocation::HID);
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
