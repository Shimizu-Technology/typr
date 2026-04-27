use crate::logger;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

const TYPR_BUNDLE_ID: &str = "com.shimizutechnology.typr";

#[derive(Debug, Clone)]
pub struct FocusTarget {
    pub app_name: String,
    pub bundle_id: String,
    #[cfg(target_os = "macos")]
    pub pid: i32,
}

#[cfg(target_os = "macos")]
pub fn capture_focus_target() -> Option<FocusTarget> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let bundle_id = app.bundleIdentifier()?.to_string();
    let pid = app.processIdentifier();

    if bundle_id == TYPR_BUNDLE_ID || pid <= 0 {
        return None;
    }

    let app_name = app
        .localizedName()
        .map(|name| name.to_string())
        .unwrap_or_else(|| bundle_id.clone());
    logger::info(&format!(
        "[Typr] Captured focus target: {} ({}) pid={}",
        app_name, bundle_id, pid
    ));

    Some(FocusTarget {
        app_name,
        bundle_id,
        pid,
    })
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
pub fn restore_focus_target(target: &FocusTarget) -> Result<(), String> {
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(target.pid)
    else {
        return Err(format!(
            "No running application found for pid {} ({})",
            target.pid, target.bundle_id
        ));
    };

    let unhide_ok = app.unhide();
    let activate_ok =
        app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);

    logger::info(&format!(
        "[Typr] Restore focus target pid={} bundle={} unhide_ok={} activate_ok={}",
        target.pid, target.bundle_id, unhide_ok, activate_ok
    ));

    if !activate_ok {
        Err(format!(
            "macOS denied activation for pid {} ({})",
            target.pid, target.bundle_id
        ))
    } else {
        for attempt in 1..=20 {
            let Some(frontmost) = current_frontmost_target() else {
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            };

            logger::info(&format!(
                "[Typr] Focus check attempt {}: frontmost {} ({}) pid={}",
                attempt, frontmost.app_name, frontmost.bundle_id, frontmost.pid
            ));

            if frontmost.pid == target.pid {
                return Ok(());
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let current = current_frontmost_target()
            .map(|frontmost| {
                format!(
                    "frontmost is {} ({}) pid={}",
                    frontmost.app_name, frontmost.bundle_id, frontmost.pid
                )
            })
            .unwrap_or_else(|| "frontmost app could not be determined".to_string());

        Err(format!(
            "Target app never became frontmost after activation; {}",
            current
        ))
    }
}

#[cfg(target_os = "macos")]
fn current_frontmost_target() -> Option<FocusTarget> {
    let workspace = NSWorkspace::sharedWorkspace();
    let app = workspace.frontmostApplication()?;
    let bundle_id = app.bundleIdentifier()?.to_string();
    let pid = app.processIdentifier();
    let app_name = app
        .localizedName()
        .map(|name| name.to_string())
        .unwrap_or_else(|| bundle_id.clone());

    Some(FocusTarget {
        app_name,
        bundle_id,
        pid,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn capture_focus_target() -> Option<FocusTarget> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn restore_focus_target(_target: &FocusTarget) -> Result<(), String> {
    Ok(())
}
