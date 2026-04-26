use std::thread;
use std::time::Duration;

pub fn capture_selected_text() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let previous_text = clipboard.get_text().ok();
    let marker = format!(
        "__typr_selection_probe_{}__",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );

    clipboard
        .set_text(marker.clone())
        .map_err(|e| format!("Failed to prepare clipboard for selection capture: {}", e))?;

    let capture_result = (|| -> Result<String, String> {
        trigger_copy_shortcut()?;
        thread::sleep(Duration::from_millis(120));

        let clipboard_text = clipboard
            .get_text()
            .map_err(|e| format!("Failed to read selected text from clipboard: {}", e))?;

        validate_selected_text(&clipboard_text, &marker)
    })();

    restore_previous_clipboard_text(&mut clipboard, previous_text);
    capture_result
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
        let click_result = enigo
            .key(Key::Unicode('c'), Direction::Click)
            .map_err(|e| e.to_string());
        let release_result = enigo
            .key(Key::Control, Direction::Release)
            .map_err(|e| e.to_string());

        match (click_result, release_result) {
            (Err(click_err), Err(release_err)) => {
                return Err(format!(
                    "Copy keypress failed: {}. Also failed to release Control: {}",
                    click_err, release_err
                ));
            }
            (Err(click_err), Ok(())) => return Err(click_err),
            (Ok(()), Err(release_err)) => return Err(release_err),
            (Ok(()), Ok(())) => {}
        }
    }

    Ok(())
}

fn restore_previous_clipboard_text(
    clipboard: &mut arboard::Clipboard,
    previous_text: Option<String>,
) {
    match previous_text {
        Some(previous_text) => {
            let _ = clipboard.set_text(previous_text);
        }
        None => {
            let _ = clipboard.set_text(String::new());
        }
    }
}

fn validate_selected_text(clipboard_text: &str, marker: &str) -> Result<String, String> {
    if clipboard_text == marker {
        return Err(
            "No selected text found. Highlight text first, then try command mode again."
                .to_string(),
        );
    }

    let trimmed = clipboard_text.trim();
    if trimmed.is_empty() {
        return Err(
            "No selected text found. Highlight text first, then try command mode again."
                .to_string(),
        );
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_means_no_new_selection() {
        let err = validate_selected_text("__marker__", "__marker__").unwrap_err();
        assert!(err.contains("Highlight text first"));
    }

    #[test]
    fn trims_real_selection() {
        let selected = validate_selected_text("  hello world  ", "__marker__").unwrap();
        assert_eq!(selected, "hello world");
    }
}
