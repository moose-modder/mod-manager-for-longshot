//! Whether Longshot or Hookshot is running. They read their mods only when they start, so changes made while one runs
//! show after a restart. (An emulator isn't looked for.)

use std::time::Duration;

use crate::game::PROGRAMS;

/// How often to look.
pub const EVERY: Duration = Duration::from_secs(3);

/// The program running, of [`PROGRAMS`]: a process named `longshot.exe` or `hookshot.exe` (as Task Manager lists
/// it).
#[cfg(windows)]
pub fn running() -> Option<&'static str> {
    use std::os::windows::process::CommandExt;
    /// No console window flashing up for `tasklist` (the app has none).
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("tasklist")
        .args(["/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    in_task_list(&String::from_utf8_lossy(&out.stdout))
}

/// The program of [`PROGRAMS`] in `tasklist /FO CSV /NH`'s lines (each starts with the quoted image name).
#[cfg_attr(not(windows), allow(dead_code))]
fn in_task_list(list: &str) -> Option<&'static str> {
    let names: Vec<String> = list
        .lines()
        .filter_map(|l| l.split(',').next())
        .map(|n| n.trim_matches('"').to_ascii_lowercase())
        .collect();
    PROGRAMS.iter().map(|p| p.name).find(|name| {
        let exe = format!("{}.exe", name.to_ascii_lowercase());
        names.contains(&exe)
    })
}

/// The program running, of [`PROGRAMS`]: a process named `longshot` or `hookshot` in /proc (or `longshot.exe`,
/// `hookshot.exe`, as Wine runs them).
#[cfg(not(windows))]
pub fn running() -> Option<&'static str> {
    let entries = std::fs::read_dir("/proc").ok()?;
    let names: Vec<String> = entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .map(|c| c.trim().to_ascii_lowercase())
        .collect();
    PROGRAMS.iter().map(|p| p.name).find(|name| {
        let name = name.to_ascii_lowercase();
        names
            .iter()
            .any(|n| *n == name || n.strip_suffix(".exe") == Some(name.as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn found_in_the_task_list() {
        let list = "\"System\",\"4\",\"Services\",\"0\",\"144 K\"\r\n\
                    \"Hookshot.exe\",\"9120\",\"Console\",\"1\",\"812,004 K\"\r\n";
        assert_eq!(in_task_list(list), Some("Hookshot"));
        assert_eq!(
            in_task_list("\"longshot.exe\",\"1\",\"Console\",\"1\",\"1 K\""),
            Some("Longshot")
        );
        assert_eq!(
            in_task_list("\"longshot_launcher.exe\",\"1\",\"Console\",\"1\",\"1 K\""),
            None
        );
        assert_eq!(in_task_list(""), None);
    }
}
