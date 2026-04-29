use crate::focus::{restore_focus_target, FocusTarget};
use crate::logger;
#[cfg(target_os = "macos")]
use crate::macos_input::{release_common_modifiers, send_command_shortcut};
use tauri::AppHandle;

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
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut Boolean,
    ) -> AXError;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithCString(
        alloc: *const std::ffi::c_void,
        c_str: *const std::ffi::c_char,
        encoding: u32,
    ) -> CFStringRef;
    fn CFRelease(cf: CFTypeRef);
    static kCFBooleanTrue: CFTypeRef;
}

#[cfg(target_os = "macos")]
type AXUIElementRef = *const std::ffi::c_void;
#[cfg(target_os = "macos")]
type CFTypeRef = *const std::ffi::c_void;
#[cfg(target_os = "macos")]
type CFStringRef = *const std::ffi::c_void;
#[cfg(target_os = "macos")]
type AXError = i32;
#[cfg(target_os = "macos")]
type Boolean = u8;

#[cfg(target_os = "macos")]
const AX_SUCCESS: AXError = 0;
#[cfg(target_os = "macos")]
const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

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
                detail: "Paste automation is blocked. Enable Accessibility access for the current Typr.app build before Typr can paste text into other apps automatically.".to_string(),
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
    focus_target: Option<&FocusTarget>,
) -> Result<TextDeliveryResult, String> {
    write_clipboard_text_verified(text)?;

    if let Some(focus_target) = focus_target {
        #[cfg(target_os = "macos")]
        {
            if let Err(err) = restore_focus_target(focus_target) {
                logger::error(&format!(
                    "[Typr] Focus restore failed for {} (pid={}): {}",
                    focus_target.bundle_id, focus_target.pid, err
                ));
            } else {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
    }

    let mut delivery_error: Option<String> = None;
    let mut verified_delivery = false;

    // On macOS, the real paste path is more reliable than AXSelectedText for
    // editor-like apps such as Obsidian and Electron shells.
    #[cfg(target_os = "macos")]
    {
        let trusted = unsafe { AXIsProcessTrusted() };
        logger::info(&format!(
            "[Typr] macOS Accessibility trusted before paste delivery: {}",
            trusted
        ));
        log_macos_accessibility_context(focus_target);

        if !trusted {
            logger::error(
                "[Typr] Automatic paste skipped because macOS Accessibility is not trusted",
            );
            return Ok(TextDeliveryResult {
                delivered_to_cursor: false,
                detail: "Typr copied the text to your clipboard, but macOS is blocking automatic paste. Enable Accessibility access for the current Typr.app build, then restart Typr and try again.".to_string(),
            });
        }

        logger::info("[Typr] Triggering native macOS paste shortcut");
        let target_pid = focus_target.map(|target| target.pid);
        if let Err(paste_err) = run_macos_shortcut_with_retry(app, 'v', target_pid, 3) {
            logger::error(&format!(
                "[Typr] Native paste shortcut failed: {}",
                paste_err
            ));

            let ax_insert_result = if trusted {
                logger::info("[Typr] Trying accessibility text insertion fallback");
                run_macos_accessibility_insert(text, focus_target)
            } else {
                Err("macOS Accessibility API is disabled for this Typr binary".to_string())
            };

            if let Err(ax_err) = ax_insert_result {
                logger::error(&format!(
                    "[Typr] Accessibility text insertion fallback failed: {}",
                    ax_err
                ));
                logger::info("[Typr] Falling back to native macOS text insertion");
                if let Err(text_err) = run_macos_text_input(app, text) {
                    logger::error(&format!(
                        "[Typr] Direct text insertion failed: {}",
                        text_err
                    ));
                    delivery_error = Some(format!(
                        "Accessibility insertion failed: {}. Paste shortcut failed: {}. Direct text insertion also failed: {}",
                        ax_err, paste_err, text_err
                    ));
                }
            } else {
                verified_delivery = true;
            }
        } else {
            logger::info("[Typr] Native macOS paste shortcut completed");
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
                delivery_error = Some(format!(
                    "Paste keypress failed: {}. Also failed to release Control: {}",
                    click_err, release_err
                ));
            }
            (Err(click_err), Ok(())) => delivery_error = Some(click_err),
            (Ok(()), Err(release_err)) => delivery_error = Some(release_err),
            (Ok(()), Ok(())) => {}
        }
    }

    if let Some(err) = delivery_error {
        return Ok(TextDeliveryResult {
            delivered_to_cursor: false,
            detail: format!(
                "Typr copied the text to your clipboard, but could not inject it at the cursor: {}",
                err
            ),
        });
    }

    #[cfg(target_os = "macos")]
    if !verified_delivery {
        return Ok(TextDeliveryResult {
            delivered_to_cursor: false,
            detail: "Typr copied the text to your clipboard and sent paste to the target app, but macOS does not provide a reliable way to verify that the target accepted it.".to_string(),
        });
    }

    Ok(TextDeliveryResult {
        delivered_to_cursor: true,
        detail:
            "Typr inserted the text at the active cursor and left the same text on your clipboard."
                .to_string(),
    })
}

#[cfg(target_os = "macos")]
fn log_macos_accessibility_context(focus_target: Option<&FocusTarget>) {
    let current_exe = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|err| format!("unavailable ({})", err));
    let current_dir = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|err| format!("unavailable ({})", err));
    let process_id = std::process::id();

    logger::info(&format!(
        "[Typr] Accessibility context: pid={} exe='{}' cwd='{}'",
        process_id, current_exe, current_dir
    ));

    if let Some(target) = focus_target {
        logger::info(&format!(
            "[Typr] Accessibility context target: {} ({}) pid={}",
            target.app_name, target.bundle_id, target.pid
        ));
    } else {
        logger::info("[Typr] Accessibility context target: none captured");
    }
}

fn write_clipboard_text_verified(text: &str) -> Result<(), String> {
    let expected = text.to_string();
    let mut last_error = None;

    for attempt in 1..=5 {
        match write_clipboard_text_once(&expected) {
            Ok(()) => {
                std::thread::sleep(std::time::Duration::from_millis(40));
                match read_clipboard_text_once() {
                    Ok(actual) if actual == expected => {
                        logger::info(&format!(
                            "[Typr] Clipboard write verified on attempt {} ({} chars)",
                            attempt,
                            expected.chars().count()
                        ));
                        return Ok(());
                    }
                    Ok(actual) => {
                        last_error = Some(format!(
                            "Clipboard verification mismatch on attempt {}: expected {} chars, read {} chars",
                            attempt,
                            expected.chars().count(),
                            actual.chars().count()
                        ));
                    }
                    Err(err) => {
                        last_error = Some(format!(
                            "Clipboard readback failed on attempt {}: {}",
                            attempt, err
                        ));
                    }
                }
            }
            Err(err) => {
                last_error = Some(format!(
                    "Clipboard write failed on attempt {}: {}",
                    attempt, err
                ));
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    Err(format!(
        "Clipboard write could not be verified: {}",
        last_error.unwrap_or_else(|| "unknown clipboard failure".to_string())
    ))
}

fn write_clipboard_text_once(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

fn read_clipboard_text_once() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn run_macos_accessibility_insert(
    text: &str,
    focus_target: Option<&FocusTarget>,
) -> Result<(), String> {
    let mut accessibility_bootstrap = None;
    if let Some(target) = focus_target {
        accessibility_bootstrap = enable_target_accessibility(target);
    }

    let focused_element = unsafe {
        let app_element = focus_target.and_then(|target| {
            if target.pid <= 0 {
                None
            } else {
                Some(AXUIElementCreateApplication(target.pid))
            }
        });
        let system_wide = AXUIElementCreateSystemWide();
        let focused_attr = cfstring("AXFocusedUIElement")?;
        let mut focused_value: CFTypeRef = std::ptr::null();
        let focused_result = if let Some(app_element) = app_element {
            let (result, value) = copy_focused_element_with_retry(
                app_element,
                focus_target.expect("app element implies focus target").pid,
                focused_attr,
            );
            focused_value = value;
            if result == AX_SUCCESS && !focused_value.is_null() {
                result
            } else {
                let fallback_result =
                    AXUIElementCopyAttributeValue(system_wide, focused_attr, &mut focused_value);
                logger::info(&format!(
                    "[Typr] AXFocusedUIElement via system-wide fallback result={}",
                    fallback_result
                ));
                fallback_result
            }
        } else {
            AXUIElementCopyAttributeValue(system_wide, focused_attr, &mut focused_value)
        };
        CFRelease(focused_attr);
        CFRelease(system_wide as CFTypeRef);
        if let Some(app_element) = app_element {
            CFRelease(app_element as CFTypeRef);
        }

        if focused_result != AX_SUCCESS || focused_value.is_null() {
            if let Some(bootstrap_error) = accessibility_bootstrap {
                logger::error(&format!(
                    "[Typr] Accessibility bootstrap note: {}",
                    bootstrap_error
                ));
            }
            return Err(format!(
                "Could not read focused UI element (AXError={})",
                focused_result
            ));
        }

        focused_value as AXUIElementRef
    };

    let selected_text_attr = cfstring("AXSelectedText")?;
    let mut selected_text_settable: Boolean = 0;
    let selected_text_settable_result = unsafe {
        AXUIElementIsAttributeSettable(
            focused_element,
            selected_text_attr,
            &mut selected_text_settable,
        )
    };

    logger::info(&format!(
        "[Typr] AXSelectedText settable check result={} settable={}",
        selected_text_settable_result, selected_text_settable
    ));

    if selected_text_settable_result == AX_SUCCESS && selected_text_settable != 0 {
        let replacement = cfstring(text)?;
        let set_result = unsafe {
            AXUIElementSetAttributeValue(focused_element, selected_text_attr, replacement)
        };
        unsafe {
            CFRelease(replacement);
            CFRelease(selected_text_attr);
            CFRelease(focused_element as CFTypeRef);
        }

        if set_result == AX_SUCCESS {
            logger::info("[Typr] Accessibility text insertion succeeded via AXSelectedText");
            return Ok(());
        }

        return Err(format!(
            "AXSelectedText write failed with AXError={}",
            set_result
        ));
    }

    unsafe {
        CFRelease(selected_text_attr);
        CFRelease(focused_element as CFTypeRef);
    }

    Err("Focused element does not expose settable AXSelectedText".to_string())
}

#[cfg(target_os = "macos")]
fn copy_focused_element_with_retry(
    app_element: AXUIElementRef,
    pid: i32,
    focused_attr: CFStringRef,
) -> (AXError, CFTypeRef) {
    let mut last_result = AX_SUCCESS;
    let mut focused_value: CFTypeRef = std::ptr::null();

    for attempt in 1..=5 {
        focused_value = std::ptr::null();
        last_result =
            unsafe { AXUIElementCopyAttributeValue(app_element, focused_attr, &mut focused_value) };
        logger::info(&format!(
            "[Typr] AXFocusedUIElement via target app pid={} attempt={} result={} hasValue={}",
            pid,
            attempt,
            last_result,
            !focused_value.is_null()
        ));

        if last_result == AX_SUCCESS && !focused_value.is_null() {
            return (last_result, focused_value);
        }

        if last_result == -25211 {
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    (last_result, focused_value)
}

#[cfg(target_os = "macos")]
fn enable_target_accessibility(target: &FocusTarget) -> Option<String> {
    if target.pid <= 0 {
        return None;
    }

    let manual_attr = match cfstring("AXManualAccessibility") {
        Ok(value) => value,
        Err(err) => return Some(err),
    };
    let enhanced_attr = match cfstring("AXEnhancedUserInterface") {
        Ok(value) => value,
        Err(err) => {
            unsafe {
                CFRelease(manual_attr);
            }
            return Some(err);
        }
    };
    let app_element = unsafe { AXUIElementCreateApplication(target.pid) };
    if app_element.is_null() {
        unsafe {
            CFRelease(manual_attr);
            CFRelease(enhanced_attr);
        }
        return Some(format!(
            "Could not create AX application element for pid {} ({})",
            target.pid, target.bundle_id
        ));
    }

    let manual_result =
        unsafe { AXUIElementSetAttributeValue(app_element, manual_attr, kCFBooleanTrue) };
    logger::info(&format!(
        "[Typr] AXManualAccessibility enable pid={} bundle={} result={}",
        target.pid, target.bundle_id, manual_result
    ));
    let enhanced_result =
        unsafe { AXUIElementSetAttributeValue(app_element, enhanced_attr, kCFBooleanTrue) };
    logger::info(&format!(
        "[Typr] AXEnhancedUserInterface enable pid={} bundle={} result={}",
        target.pid, target.bundle_id, enhanced_result
    ));

    unsafe {
        CFRelease(manual_attr);
        CFRelease(enhanced_attr);
        CFRelease(app_element as CFTypeRef);
    }

    if manual_result == AX_SUCCESS || enhanced_result == AX_SUCCESS {
        // Chromium/Electron can enable native accessibility asynchronously after this signal.
        std::thread::sleep(std::time::Duration::from_millis(400));
        None
    } else {
        Some(format!(
            "AXManualAccessibility result={}, AXEnhancedUserInterface result={}",
            manual_result, enhanced_result
        ))
    }
}

#[cfg(target_os = "macos")]
fn run_macos_shortcut_with_retry(
    app: &AppHandle,
    key: char,
    target_pid: Option<i32>,
    attempts: usize,
) -> Result<(), String> {
    let mut last_error = None;

    for attempt in 1..=attempts {
        logger::info(&format!(
            "[Typr] Native paste attempt {} of {}",
            attempt, attempts
        ));

        match send_command_shortcut(app, key, target_pid) {
            Ok(()) => return Ok(()),
            Err(err) => {
                last_error = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "Unknown macOS paste shortcut failure".to_string()))
}

#[cfg(target_os = "macos")]
fn run_macos_text_input(app: &AppHandle, text: &str) -> Result<(), String> {
    use enigo::{Enigo, Keyboard, Settings};
    use std::sync::mpsc;

    let text = text.to_string();
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
            release_common_modifiers()?;
            std::thread::sleep(std::time::Duration::from_millis(50));
            enigo.text(&text).map_err(|e| e.to_string())
        })();

        let _ = tx.send(result);
    })
    .map_err(|e| format!("Failed to schedule macOS text insertion: {}", e))?;

    rx.recv()
        .map_err(|e| format!("Failed to receive macOS text insertion result: {}", e))?
}

#[cfg(target_os = "macos")]
fn cfstring(value: &str) -> Result<CFStringRef, String> {
    let value = std::ffi::CString::new(value)
        .map_err(|_| "String contained an interior NUL byte".to_string())?;
    let string = unsafe {
        CFStringCreateWithCString(std::ptr::null(), value.as_ptr(), K_CF_STRING_ENCODING_UTF8)
    };

    if string.is_null() {
        Err("Failed to allocate CFString".to_string())
    } else {
        Ok(string)
    }
}
