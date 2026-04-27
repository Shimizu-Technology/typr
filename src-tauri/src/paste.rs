use tauri::AppHandle;
use crate::focus::{restore_focus_target, FocusTarget};

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
    focus_target: Option<&FocusTarget>,
) -> Result<TextDeliveryResult, String> {
    // Set clipboard (arboard is thread-safe)
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;

    // Small delay to ensure clipboard is set
    std::thread::sleep(std::time::Duration::from_millis(50));

    if let Some(focus_target) = focus_target {
        #[cfg(target_os = "macos")]
        {
            if let Err(err) = restore_focus_target(focus_target) {
                eprintln!(
                    "[Typr] Focus restore failed for {} (pid={}): {}",
                    focus_target.bundle_id, focus_target.pid, err
                );
            } else {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }
    }

    let mut delivery_error: Option<String> = None;

    // On macOS, prefer direct text insertion at the focused cursor.
    #[cfg(target_os = "macos")]
    {
        println!("[Typr] Trying accessibility text insertion");
        if let Err(ax_err) = run_macos_accessibility_insert(text, focus_target) {
            eprintln!("[Typr] Accessibility text insertion failed: {}", ax_err);
            println!("[Typr] Triggering native macOS text insertion");
            if let Err(text_err) = run_macos_text_input(app, text) {
                eprintln!("[Typr] Direct text insertion failed: {}", text_err);
                println!("[Typr] Falling back to native macOS paste shortcut");
                if let Err(paste_err) = run_macos_shortcut(app, 'v') {
                    delivery_error = Some(format!(
                        "Accessibility insertion failed: {}. Direct text insertion failed: {}. Paste automation also failed: {}",
                        ax_err, text_err, paste_err
                    ));
                }
            }
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

    Ok(TextDeliveryResult {
        delivered_to_cursor: true,
        detail: "Typr inserted the text at the active cursor and left the same text on your clipboard.".to_string(),
    })
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
                println!(
                    "[Typr] AXFocusedUIElement via system-wide fallback result={}",
                    fallback_result
                );
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
                eprintln!("[Typr] Accessibility bootstrap note: {}", bootstrap_error);
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

    println!(
        "[Typr] AXSelectedText settable check result={} settable={}",
        selected_text_settable_result, selected_text_settable
    );

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
            println!("[Typr] Accessibility text insertion succeeded via AXSelectedText");
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

    for attempt in 1..=20 {
        focused_value = std::ptr::null();
        last_result =
            unsafe { AXUIElementCopyAttributeValue(app_element, focused_attr, &mut focused_value) };
        println!(
            "[Typr] AXFocusedUIElement via target app pid={} attempt={} result={} hasValue={}",
            pid,
            attempt,
            last_result,
            !focused_value.is_null()
        );

        if last_result == AX_SUCCESS && !focused_value.is_null() {
            return (last_result, focused_value);
        }

        std::thread::sleep(std::time::Duration::from_millis(120));
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
    println!(
        "[Typr] AXManualAccessibility enable pid={} bundle={} result={}",
        target.pid, target.bundle_id, manual_result
    );
    let enhanced_result =
        unsafe { AXUIElementSetAttributeValue(app_element, enhanced_attr, kCFBooleanTrue) };
    println!(
        "[Typr] AXEnhancedUserInterface enable pid={} bundle={} result={}",
        target.pid, target.bundle_id, enhanced_result
    );

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

#[cfg(target_os = "macos")]
fn run_macos_text_input(app: &AppHandle, text: &str) -> Result<(), String> {
    use enigo::{Enigo, Keyboard, Settings};
    use std::sync::mpsc;

    let text = text.to_string();
    let (tx, rx) = mpsc::channel();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
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
    let value =
        std::ffi::CString::new(value).map_err(|_| "String contained an interior NUL byte".to_string())?;
    let string = unsafe {
        CFStringCreateWithCString(
            std::ptr::null(),
            value.as_ptr(),
            K_CF_STRING_ENCODING_UTF8,
        )
    };

    if string.is_null() {
        Err("Failed to allocate CFString".to_string())
    } else {
        Ok(string)
    }
}
