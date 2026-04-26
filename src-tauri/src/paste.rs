#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastePermissionStatus {
    pub supported: bool,
    pub ready: bool,
    pub detail: String,
}

pub fn check_paste_permission_status() -> PastePermissionStatus {
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("osascript")
            .args([
                "-e",
                r#"tell application "System Events" to get name of first application process whose frontmost is true"#,
            ])
            .output()
        {
            Ok(output) if output.status.success() => PastePermissionStatus {
                supported: true,
                ready: true,
                detail: "Accessibility automation looks available for paste injection."
                    .to_string(),
            },
            Ok(output) => PastePermissionStatus {
                supported: true,
                ready: false,
                detail: format!(
                    "Paste automation is blocked. macOS usually needs Accessibility access for Typr. {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            },
            Err(err) => PastePermissionStatus {
                supported: true,
                ready: false,
                detail: format!("Failed to run paste permission check: {}", err),
            },
        }
    }

    #[cfg(target_os = "windows")]
    {
        PastePermissionStatus {
            supported: true,
            ready: true,
            detail: "Windows paste injection uses keyboard simulation and does not need a separate permission check here.".to_string(),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        PastePermissionStatus {
            supported: false,
            ready: false,
            detail: "Paste permission checks are not implemented for this platform yet."
                .to_string(),
        }
    }
}

pub fn paste_text(text: &str) -> Result<(), String> {
    // Set clipboard (arboard is thread-safe)
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;

    // Small delay to ensure clipboard is set
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Simulate Cmd+V via osascript (works from any thread, unlike enigo which
    // calls TSMGetInputSourceProperty requiring the main thread)
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                r#"tell application "System Events" to keystroke "v" using command down"#,
            ])
            .output()
            .map_err(|e| format!("Failed to simulate paste: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Paste automation failed: {}", stderr.trim()));
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
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(|e| e.to_string());
        let release_result = enigo
            .key(Key::Control, Direction::Release)
            .map_err(|e| e.to_string());

        match (click_result, release_result) {
            (Err(click_err), Err(release_err)) => {
                return Err(format!(
                    "Paste keypress failed: {}. Also failed to release Control: {}",
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
