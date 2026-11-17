//! Mod Manager for Longshot: Longshot's mods folder, its mods on and off, and one resource size table for every
//! mod that is on, rebuilt whenever the mods folder changes. The mods folder's rules and the table come from
//! `modstudio-mods`, shared with Hookshot Mod Studio, so both build the same table. The window is a web page
//! (`src/`); the table's state reaches it as `table` events.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod conflicts;
mod game;
mod install;
mod running;
mod watch;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use modstudio_mods::mods::{self, InstalledMod, Stamp};
use modstudio_mods::progress::{Event, Progress};
use notify::RecommendedWatcher;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use game::Game;
use watch::{Phase, Request};

/// This app and its version, as the size table's stamp names the app that built it.
const MADE_BY: &str = concat!("Mod Manager for Longshot ", env!("CARGO_PKG_VERSION"));

/// Errors reach the page as their message.
type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// What's kept between runs: `settings.json` in the app's config folder, or the file `MODMANAGER_SETTINGS` names
/// (to try things without changing the real settings).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Settings {
    /// The game folder, found again at each start.
    #[serde(default)]
    game_dir: Option<PathBuf>,
}

impl Settings {
    fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(err)?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum TableState {
    /// Nothing done yet.
    #[default]
    Idle,
    /// The mods folder changed: a rebuild follows when it's quiet.
    Waiting,
    Building,
    /// Built for the mods that are on.
    Ready,
    /// No mod needs one (none on, or every file fits the game's sizes).
    None,
    Error,
}

/// The size table's state, for the page.
#[derive(Debug, Clone, Default, Serialize)]
struct TableStatus {
    state: TableState,
    message: String,
    /// The last rebuild's lines.
    log: Vec<String>,
    warnings: Vec<String>,
    /// When the last rebuild finished (seconds since 1970).
    finished: Option<u64>,
}

struct App {
    settings_path: PathBuf,
    game: Arc<Mutex<Option<Game>>>,
    /// Why the game folder chosen before can't be used (it moved, or was removed).
    problem: Mutex<Option<String>>,
    status: Arc<Mutex<TableStatus>>,
    requests: Mutex<Sender<Request>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// Whether Longshot is running (looked at every few seconds).
    running: Arc<AtomicBool>,
}

impl App {
    fn game(&self) -> Result<Game> {
        self.game
            .lock()
            .expect("game lock")
            .clone()
            .ok_or_else(|| "no Longshot folder chosen yet".to_owned())
    }

    fn request(&self, request: Request) {
        let _ = self.requests.lock().expect("requests lock").send(request);
    }

    /// Uses `game`: its mods folder made if it isn't there, watched, and the table rebuilt.
    fn use_game(&self, game: Game) -> Result<()> {
        std::fs::create_dir_all(&game.mods)
            .map_err(|e| format!("making {}: {e}", game.mods.display()))?;
        let to = self.requests.lock().expect("requests lock").clone();
        let watcher = watch::watch(&game.mods, to)
            .map_err(|e| format!("watching {}: {e}", game.mods.display()))?;
        *self.watcher.lock().expect("watcher lock") = Some(watcher);
        *self.game.lock().expect("game lock") = Some(game);
        *self.problem.lock().expect("problem lock") = None;
        self.request(Request::Now);
        Ok(())
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "the mod that's on", "the 3 mods that are on".
fn the_mods_on(n: usize) -> String {
    if n == 1 {
        "the mod that's on".into()
    } else {
        format!("the {n} mods that are on")
    }
}

/// `text` with a capital first letter (the library's messages start lower case, to follow a program's name).
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The worker's phases: the table rebuilt for the game's mods, its state kept and sent to the page.
fn on_phase(
    app: &AppHandle,
    game: &Mutex<Option<Game>>,
    status: &Mutex<TableStatus>,
    phase: Phase,
) {
    let set = |s: TableStatus| {
        *status.lock().expect("status lock") = s.clone();
        let _ = app.emit("table", s);
    };
    let Some(game) = game.lock().expect("game lock").clone() else {
        return;
    };
    match phase {
        Phase::Waiting => set(TableStatus {
            state: TableState::Waiting,
            message: "The mods folder changed: the size table follows in a moment".into(),
            ..TableStatus::default()
        }),
        Phase::Rebuild => {
            set(TableStatus {
                state: TableState::Building,
                message: "Working out what every mod that is on needs".into(),
                ..TableStatus::default()
            });
            let (mut log, mut warnings) = (Vec::new(), Vec::new());
            let result = {
                let mut progress = Progress::new(|e| match e {
                    Event::Step(s) | Event::Line(s) => log.push(s),
                    Event::Warning(w) => warnings.push(w),
                });
                mods::rebuild_size_table(&game.mods, &game.romfs, MADE_BY, &mut progress)
            };
            let on = mods::list(&game.mods)
                .map(|m| m.iter().filter(|m| m.enabled).count())
                .unwrap_or(0);
            let (state, message) = match result {
                Ok(Some(_)) => (
                    TableState::Ready,
                    format!("One size table for {}", the_mods_on(on)),
                ),
                Ok(None) if on == 0 => (TableState::None, "No mods are on".into()),
                Ok(None) => (
                    TableState::None,
                    format!(
                        "Every file of {} fits the game's own sizes",
                        the_mods_on(on)
                    ),
                ),
                Err(e) => (TableState::Error, sentence(&e.to_string())),
            };
            set(TableStatus {
                state,
                message,
                log,
                warnings,
                finished: Some(now()),
            });
        }
    }
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
struct StateView {
    game: Option<Game>,
    problem: Option<String>,
    status: TableStatus,
    settings_path: PathBuf,
    made_by: &'static str,
    /// Longshot is running: changes show after a restart.
    running: bool,
}

fn state_view(state: &App) -> StateView {
    StateView {
        game: state.game.lock().expect("game lock").clone(),
        problem: state.problem.lock().expect("problem lock").clone(),
        status: state.status.lock().expect("status lock").clone(),
        settings_path: state.settings_path.clone(),
        made_by: MADE_BY,
        running: state.running.load(Ordering::Relaxed),
    }
}

#[tauri::command]
fn get_state(state: State<'_, App>) -> StateView {
    state_view(&state)
}

/// Uses the Longshot folder the user picked: the launcher's folder, the game folder, or its romfs or mods folder.
#[tauri::command]
async fn choose_game_folder(state: State<'_, App>, dir: PathBuf) -> Result<StateView> {
    let game = game::find(&dir)?;
    state.use_game(game.clone())?;
    Settings {
        game_dir: Some(game.dir),
    }
    .save(&state.settings_path)?;
    Ok(state_view(&state))
}

#[derive(Serialize)]
struct ModsView {
    mods: Vec<InstalledMod>,
    /// The size table installed for all mods, if there is one.
    table: Option<PathBuf>,
    stamp: Option<Stamp>,
    /// Mods that change the same files, between the mods that are on.
    conflicts: Vec<conflicts::Conflict>,
}

#[tauri::command]
async fn list_mods(state: State<'_, App>) -> Result<ModsView> {
    let game = state.game()?;
    Ok(ModsView {
        mods: mods::list(&game.mods).map_err(err)?,
        table: mods::size_table(&game.mods),
        stamp: mods::stamp(&game.mods),
        conflicts: conflicts::conflicts(&game.mods)?,
    })
}

/// Turns a mod on or off (renames its folder); the table follows, as for any change in the mods folder.
#[tauri::command]
async fn set_mod_enabled(state: State<'_, App>, folder: String, enabled: bool) -> Result<String> {
    let game = state.game()?;
    let new = mods::set_enabled(&game.mods, &folder, enabled).map_err(err)?;
    state.request(Request::Changed);
    Ok(new)
}

/// Installs the mods in the zips and folders the user dropped or picked ([`install::install`]): when some are
/// there already, nothing is installed and the report names them, unless `replace`.
#[tauri::command]
async fn install_mods(
    state: State<'_, App>,
    paths: Vec<PathBuf>,
    replace: bool,
) -> Result<install::Report> {
    let game = state.game()?;
    let report =
        tauri::async_runtime::spawn_blocking(move || install::install(&game.mods, &paths, replace))
            .await
            .map_err(err)??;
    if !report.installed.is_empty() {
        state.request(Request::Changed);
    }
    Ok(report)
}

#[tauri::command]
fn rebuild_now(state: State<'_, App>) {
    state.request(Request::Now);
}

/// Opens the mods folder (`which`: "mods") or the game folder ("game") in the file manager.
#[tauri::command]
fn open_folder(state: State<'_, App>, which: String) -> Result<()> {
    let game = state.game()?;
    let dir = if which == "game" { game.dir } else { game.mods };
    let program = if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map(|_| ())
        .map_err(err)
}

fn main() {
    let (to_worker, requests) = mpsc::channel();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let settings_path = match std::env::var_os("MODMANAGER_SETTINGS") {
                Some(path) => PathBuf::from(path),
                None => app.path().app_config_dir()?.join("settings.json"),
            };
            let state = App {
                settings_path,
                game: Arc::default(),
                problem: Mutex::new(None),
                status: Arc::default(),
                requests: Mutex::new(to_worker),
                watcher: Mutex::new(None),
                running: Arc::default(),
            };
            let (handle, game, status) = (
                app.handle().clone(),
                state.game.clone(),
                state.status.clone(),
            );
            std::thread::spawn(move || {
                watch::worker(requests, watch::QUIET, |phase| {
                    on_phase(&handle, &game, &status, phase)
                })
            });
            // Longshot running or not: the page is told when that changes. It reads its mods when it starts,
            // so when it stops the table is made again (one it held open may not have been written).
            let (handle, running, to) = (
                app.handle().clone(),
                state.running.clone(),
                state.requests.lock().expect("requests lock").clone(),
            );
            std::thread::spawn(move || {
                loop {
                    let now = running::longshot_running();
                    if running.swap(now, Ordering::Relaxed) != now {
                        let _ = handle.emit("game", now);
                        if !now {
                            let _ = to.send(Request::Now);
                        }
                    }
                    std::thread::sleep(running::EVERY);
                }
            });
            if let Some(dir) = Settings::load(&state.settings_path).game_dir
                && let Err(e) = game::find(&dir).and_then(|g| state.use_game(g))
            {
                *state.problem.lock().expect("problem lock") = Some(format!(
                    "The Longshot folder chosen before can't be used: {e}"
                ));
            }
            app.manage(state);

            let builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                    .title("Mod Manager for Longshot")
                    .inner_size(940.0, 700.0)
                    .min_inner_size(640.0, 480.0);
            // MODMANAGER_WEBVIEW_ARGS: extra WebView2 switches, e.g. "--remote-debugging-port=9334" to check
            // the page with DevTools (Windows; not set, nothing changes).
            #[cfg(windows)]
            let builder = match std::env::var("MODMANAGER_WEBVIEW_ARGS") {
                Ok(extra) => builder.additional_browser_args(&format!(
                    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection {extra}"
                )),
                Err(_) => builder,
            };
            builder.build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            choose_game_folder,
            list_mods,
            set_mod_enabled,
            install_mods,
            rebuild_now,
            open_folder,
        ])
        .run(tauri::generate_context!())
        .expect("the app's window");
}
