use std::thread;
use std::time::Duration;

pub fn capture_selected_text() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous_text = clipboard.get_text().ok();

    trigger_copy_shortcut()?;
    thread::sleep(Duration::from_millis(120));

    let selected_text = clipboard
        .get_text()
        .map_err(|e| format!("Failed to read selected text from clipboard: {}", e))?;

    if let Some(previous_text) = previous_text {
        let _ = clipboard.set_text(previous_text);
    }

    let trimmed = selected_text.trim();
    if trimmed.is_empty() {
        return Err(
            "No selected text found. Highlight text first, then try command mode again."
                .to_string(),
        );
    }

    Ok(trimmed.to_string())
}

fn trigger_copy_shortcut() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                r#"tell application "System Events" to keystroke "c" using command down"#,
            ])
            .output()
            .map_err(|e| format!("Failed to simulate copy shortcut: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Copy automation failed: {}", stderr.trim()));
        }
    }

    #[cfg(target_os = "windows")]
    {
        use enigo::{Direction, Enigo, Key, Keyboard, Settings};
        let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
        enigo
            .key(Key::Control, Direction::Press)
            .map_err(|e| e.to_string())?;
        enigo
            .key(Key::Unicode('c'), Direction::Click)
            .map_err(|e| e.to_string())?;
        enigo
            .key(Key::Control, Direction::Release)
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
