#[cfg(target_os = "macos")]
use std::sync::mpsc;
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use objc2::runtime::Bool;
#[cfg(target_os = "macos")]
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

#[cfg(target_os = "macos")]
use crate::logger;

#[cfg(target_os = "macos")]
pub fn ensure_microphone_permission() -> Result<(), String> {
    let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
        return Err("Could not load macOS audio media type for microphone permission.".to_string());
    };

    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
    logger::info(&format!(
        "[Typr] macOS microphone authorization status before recording: {}",
        describe_status(status)
    ));

    match status {
        AVAuthorizationStatus::Authorized => Ok(()),
        AVAuthorizationStatus::Denied => Err(
            "Microphone access is denied for Typr. Enable it in System Settings > Privacy & Security > Microphone."
                .to_string(),
        ),
        AVAuthorizationStatus::Restricted => Err(
            "Microphone access is restricted by macOS and cannot be used by Typr.".to_string(),
        ),
        AVAuthorizationStatus::NotDetermined => request_microphone_permission(media_type),
        other => Err(format!(
            "Unknown macOS microphone authorization status: {:?}",
            other
        )),
    }
}

#[cfg(target_os = "macos")]
fn request_microphone_permission(
    media_type: &objc2_av_foundation::AVMediaType,
) -> Result<(), String> {
    logger::info("[Typr] Requesting macOS microphone permission");
    let (tx, rx) = mpsc::channel();
    let handler = RcBlock::new(move |granted: Bool| {
        let _ = tx.send(granted.as_bool());
    });

    unsafe {
        AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
    }

    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(true) => {
            logger::info("[Typr] macOS microphone permission granted");
            Ok(())
        }
        Ok(false) => Err(
            "Microphone access was not granted for Typr. Enable it in System Settings > Privacy & Security > Microphone."
                .to_string(),
        ),
        Err(_) => Err("Timed out waiting for macOS microphone permission.".to_string()),
    }
}

#[cfg(target_os = "macos")]
fn describe_status(status: AVAuthorizationStatus) -> &'static str {
    match status {
        AVAuthorizationStatus::NotDetermined => "not_determined",
        AVAuthorizationStatus::Restricted => "restricted",
        AVAuthorizationStatus::Denied => "denied",
        AVAuthorizationStatus::Authorized => "authorized",
        _ => "unknown",
    }
}

#[cfg(not(target_os = "macos"))]
pub fn ensure_microphone_permission() -> Result<(), String> {
    Ok(())
}
