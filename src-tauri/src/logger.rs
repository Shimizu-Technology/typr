use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn log_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.shimizutechnology.typr")
        .join("debug.log")
}

fn fallback_log_path() -> PathBuf {
    std::env::temp_dir().join("typr-debug.log")
}

fn write_line(level: &str, message: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let line = format!("[{}] {} {}\n", timestamp, level, message);

    for path in [log_path(), fallback_log_path()] {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}

pub fn info(message: &str) {
    println!("{}", message);
    write_line("INFO", message);
}

pub fn error(message: &str) {
    eprintln!("{}", message);
    write_line("ERROR", message);
}

pub fn debug_log_path() -> PathBuf {
    log_path()
}

pub fn fallback_debug_log_path() -> PathBuf {
    fallback_log_path()
}

pub fn init() {
    for path in [log_path(), fallback_log_path()] {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let _ = OpenOptions::new().create(true).append(true).open(&path);
    }

    info(&format!(
        "[Typr] Debug logs initialized at {} and {}",
        log_path().display(),
        fallback_log_path().display()
    ));
}
