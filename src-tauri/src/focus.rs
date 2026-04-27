#[cfg(target_os = "macos")]
use objc2_app_kit::NSWorkspace;

const TYPR_BUNDLE_ID: &str = "com.typr.app";

#[cfg(target_os = "macos")]
pub fn capture_focus_target() -> Option<String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let bundle_id = app.bundleIdentifier()?;
    let bundle_id = bundle_id.to_string();

    if bundle_id == TYPR_BUNDLE_ID {
        None
    } else {
        Some(bundle_id)
    }
}

#[cfg(target_os = "macos")]
pub fn restore_focus_target(bundle_id: &str) -> Result<(), String> {
    let status = std::process::Command::new("open")
        .arg("-b")
        .arg(bundle_id)
        .status()
        .map_err(|err| format!("Failed to relaunch focus target {}: {}", bundle_id, err))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "open -b {} exited with status {}",
            bundle_id, status
        ))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn capture_focus_target() -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn restore_focus_target(_bundle_id: &str) -> Result<(), String> {
    Ok(())
}
