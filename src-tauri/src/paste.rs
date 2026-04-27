use tauri::AppHandle;
use crate::focus::restore_focus_target;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastePermissionStatus {
    pub supported: bool,
    pub ready: bool,
    pub detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDeliveryResult {
    pub delivered_to_cursor: bool,
    pub detail: String,
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

pub fn check_paste_permission_status() -> PastePermissionStatus {
    #[cfg(target_os = "macos")]
    {
        let trusted = unsafe { AXIsProcessTrusted() };
        if trusted {
            PastePermissionStatus {
                supported: true,
                ready: true,
                detail:
                    "Accessibility automation looks available for selection capture and paste injection."
                        .to_string(),
            }
        } else {
            PastePermissionStatus {
                supported: true,
                ready: false,
                detail: "Paste automation is blocked. Enable Accessibility access for Typr before it can copy selections or paste text into other apps.".to_string(),
            }
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

pub fn paste_text(
    app: &AppHandle,
    text: &str,
    focus_target: Option<&str>,
) -> Result<TextDeliveryResult, String> {
    // Set clipboard (arboard is thread-safe)
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;

    // Small delay to ensure clipboard is set
    std::thread::sleep(std::time::Duration::from_millis(50));

    if let Some(bundle_id) = focus_target {
        #[cfg(target_os = "macos")]
        {
            if let Err(err) = restore_focus_target(bundle_id) {
                eprintln!("[Typr] Focus restore failed for {}: {}", bundle_id, err);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
    }

    let mut paste_error: Option<String> = None;

    // Simulate Cmd+V natively on the main thread.
    #[cfg(target_os = "macos")]
    {
        if let Err(err) = run_macos_shortcut(app, 'v') {
            paste_error = Some(format!("Paste automation failed: {}", err));
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
                paste_error = Some(format!(
                    "Paste keypress failed: {}. Also failed to release Control: {}",
                    click_err, release_err
                ));
            }
            (Err(click_err), Ok(())) => paste_error = Some(click_err),
            (Ok(()), Err(release_err)) => paste_error = Some(release_err),
            (Ok(()), Ok(())) => {}
        }
    }

    if let Some(err) = paste_error {
        return Ok(TextDeliveryResult {
            delivered_to_cursor: false,
            detail: format!(
                "Typr copied the text to your clipboard, but could not inject it at the cursor: {}",
                err
            ),
        });
    }

    Ok(TextDeliveryResult {
        delivered_to_cursor: true,
        detail: "Typr inserted the text at the active cursor and left the same text on your clipboard.".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn run_macos_shortcut(app: &AppHandle, key: char) -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
            enigo
                .key(Key::Meta, Direction::Press)
                .map_err(|e| e.to_string())?;
            let click_result = enigo
                .key(Key::Unicode(key), Direction::Click)
                .map_err(|e| e.to_string());
            let release_result = enigo
                .key(Key::Meta, Direction::Release)
                .map_err(|e| e.to_string());

            match (click_result, release_result) {
                (Err(click_err), Err(release_err)) => Err(format!(
                    "{}. Also failed to release Command: {}",
                    click_err, release_err
                )),
                (Err(click_err), Ok(())) => Err(click_err),
                (Ok(()), Err(release_err)) => Err(release_err),
                (Ok(()), Ok(())) => Ok(()),
            }
        })();

        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to schedule macOS paste shortcut: {}", e))?;

    rx.recv()
        .map_err(|e| format!("Failed to receive macOS paste shortcut result: {}", e))?
}
