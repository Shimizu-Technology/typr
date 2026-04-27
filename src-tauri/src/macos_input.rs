#[cfg(target_os = "macos")]
use crate::logger;
#[cfg(target_os = "macos")]
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, KeyCode};
#[cfg(target_os = "macos")]
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
#[cfg(target_os = "macos")]
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
#[cfg(target_os = "macos")]
use std::sync::mpsc;
#[cfg(target_os = "macos")]
use tauri::AppHandle;

#[cfg(target_os = "macos")]
pub fn send_command_shortcut(app: &AppHandle, key: char) -> Result<(), String> {
    let keycode = shortcut_keycode(key)?;
    let (tx, rx) = mpsc::channel();

    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            release_common_modifiers()?;
            let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
                .map_err(|err| format!("Failed creating CGEvent source: {:?}", err))?;
            logger::info(&format!(
                "[Typr] Dispatching explicit macOS Command shortcut sequence for '{}'",
                key
            ));

            let command_down = new_keyboard_event(
                source.clone(),
                KeyCode::COMMAND,
                true,
                CGEventFlags::CGEventFlagCommand,
            )?;
            command_down.post(CGEventTapLocation::HID);

            std::thread::sleep(std::time::Duration::from_millis(12));

            let key_down = new_keyboard_event(
                source.clone(),
                keycode,
                true,
                CGEventFlags::CGEventFlagCommand,
            )?;
            key_down.post(CGEventTapLocation::HID);

            std::thread::sleep(std::time::Duration::from_millis(12));

            let key_up = new_keyboard_event(
                source.clone(),
                keycode,
                false,
                CGEventFlags::CGEventFlagCommand,
            )?;
            key_up.post(CGEventTapLocation::HID);

            std::thread::sleep(std::time::Duration::from_millis(12));

            let command_up = new_keyboard_event(
                source,
                KeyCode::COMMAND,
                false,
                CGEventFlags::CGEventFlagNull,
            )?;
            command_up.post(CGEventTapLocation::HID);

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
fn new_keyboard_event(
    source: CGEventSource,
    keycode: u16,
    keydown: bool,
    flags: CGEventFlags,
) -> Result<CGEvent, String> {
    let event = CGEvent::new_keyboard_event(source, keycode, keydown)
        .map_err(|_| "Failed creating macOS keyboard event".to_string())?;
    event.set_flags(flags);
    Ok(event)
}

#[cfg(target_os = "macos")]
fn shortcut_keycode(key: char) -> Result<u16, String> {
    let keycode = match key.to_ascii_lowercase() {
        'c' => KeyCode::ANSI_C,
        'v' => KeyCode::ANSI_V,
        other => {
            return Err(format!(
                "Unsupported macOS shortcut key '{}'; expected a known ANSI keycode mapping",
                other
            ))
        }
    };

    Ok(keycode)
}
