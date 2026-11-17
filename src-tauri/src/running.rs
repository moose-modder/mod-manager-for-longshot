//! Whether Longshot is running. It reads its mods only when it starts, so changes made while it runs show after a
//! restart.

use std::time::Duration;

/// How often to look.
pub const EVERY: Duration = Duration::from_secs(3);

/// Whether a program named `longshot.exe` is running (as Task Manager lists it).
#[cfg(windows)]
pub fn longshot_running() -> bool {
    use std::os::windows::process::CommandExt;
    /// No console window flashing up for `tasklist` (the app has none).
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq longshot.exe", "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_ascii_lowercase()
                .contains("\"longshot.exe\"")
        })
        .unwrap_or(false)
}

/// Whether a program named `longshot` is running (its name in /proc).
#[cfg(not(windows))]
pub fn longshot_running() -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|e| {
        e.file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
            && std::fs::read_to_string(e.path().join("comm")).is_ok_and(|c| c.trim() == "longshot")
    })
}
