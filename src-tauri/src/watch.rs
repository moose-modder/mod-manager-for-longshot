//! Watching the mods folder: a change (a mod copied in, removed, renamed, turned on or off) rebuilds the size
//! table once the folder has been quiet for a moment, so a mod still being copied is read whole.

use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use modstudio_mods::mods::TABLE_MOD;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// How long the mods folder must be quiet before the table is rebuilt.
pub const QUIET: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Something in the mods folder changed: rebuild once it has been quiet for a while.
    Changed,
    /// Rebuild now (the user asked, or the app started).
    Now,
}

/// What [`worker`] asks its handler to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// A change came in: a rebuild follows when the folder is quiet.
    Waiting,
    /// Rebuild the table.
    Rebuild,
}

/// Watches `mods` and everything in it, sending [`Request::Changed`] for changes outside the table's own folder
/// (the rebuild writes there) and hidden folders (an install puts a mod together in one; Longshot skips them).
/// Dropping the watcher stops it.
pub fn watch(mods: &Path, to: Sender<Request>) -> notify::Result<RecommendedWatcher> {
    let root = mods.to_owned();
    let ignored = move |p: &Path| {
        p.strip_prefix(&root)
            .ok()
            .and_then(|rel| rel.components().next())
            .is_some_and(|first| {
                let first = first.as_os_str().to_string_lossy();
                first == TABLE_MOD || first.starts_with('.')
            })
    };
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else {
            return;
        };
        let ours = !event.paths.is_empty() && event.paths.iter().all(|p| ignored(p));
        if !matches!(event.kind, EventKind::Access(_)) && !ours {
            let _ = to.send(Request::Changed);
        }
    })?;
    watcher.watch(mods, RecursiveMode::Recursive)?;
    Ok(watcher)
}

/// Takes requests until every sender is gone: [`Phase::Rebuild`] at once for [`Request::Now`], `quiet` after
/// the last [`Request::Changed`] (with [`Phase::Waiting`] when the first one comes in). Requests that come in
/// during a rebuild are taken after it, once.
pub fn worker(requests: Receiver<Request>, quiet: Duration, mut handle: impl FnMut(Phase)) {
    let mut due: Option<Instant> = None;
    loop {
        let request = match due {
            None => requests.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(at) => requests.recv_timeout(at.saturating_duration_since(Instant::now())),
        };
        match request {
            Ok(Request::Changed) => {
                if due.is_none() {
                    handle(Phase::Waiting);
                }
                due = Some(Instant::now() + quiet);
            }
            Ok(Request::Now) => due = Some(Instant::now()),
            Err(RecvTimeoutError::Timeout) => {
                due = None;
                handle(Phase::Rebuild);
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn run(
        quiet_ms: u64,
    ) -> (
        Sender<Request>,
        Receiver<Phase>,
        std::thread::JoinHandle<()>,
    ) {
        let (to_worker, requests) = mpsc::channel();
        let (to_test, phases) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker(requests, Duration::from_millis(quiet_ms), |p| {
                to_test.send(p).unwrap()
            })
        });
        (to_worker, phases, worker)
    }

    #[test]
    fn a_burst_of_changes_is_one_rebuild() {
        let (to, phases, worker) = run(150);
        for _ in 0..20 {
            to.send(Request::Changed).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
        let wait = Duration::from_secs(5);
        assert_eq!(phases.recv_timeout(wait), Ok(Phase::Waiting));
        assert_eq!(phases.recv_timeout(wait), Ok(Phase::Rebuild));
        assert!(phases.recv_timeout(Duration::from_millis(400)).is_err());
        // and "now" doesn't wait
        let asked = Instant::now();
        to.send(Request::Now).unwrap();
        assert_eq!(phases.recv_timeout(wait), Ok(Phase::Rebuild));
        assert!(asked.elapsed() < Duration::from_millis(150));
        drop(to);
        worker.join().unwrap();
    }

    #[test]
    fn changes_are_seen_but_the_tables_own_writes_are_not() {
        let dir = std::env::temp_dir().join(format!("longshot-mods-watch-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(TABLE_MOD).join("romfs")).unwrap();
        let (to, requests) = mpsc::channel();
        let watcher = watch(&dir, to).unwrap();
        std::thread::sleep(Duration::from_millis(200));

        // the table's writes, and a mod put together in a hidden folder
        std::fs::write(dir.join(TABLE_MOD).join("romfs").join("t.zs"), b"x").unwrap();
        std::fs::create_dir_all(dir.join(".install").join("BMod").join("romfs")).unwrap();
        std::fs::write(
            dir.join(".install").join("BMod").join("romfs").join("b.zs"),
            b"x",
        )
        .unwrap();
        assert!(requests.recv_timeout(Duration::from_millis(700)).is_err());

        std::fs::create_dir_all(dir.join("AMod").join("romfs")).unwrap();
        assert_eq!(
            requests.recv_timeout(Duration::from_secs(5)),
            Ok(Request::Changed)
        );
        drop(watcher);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
