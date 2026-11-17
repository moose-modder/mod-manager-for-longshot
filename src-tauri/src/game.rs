//! Longshot's game folder, found from the folder a user picks: the game's files (`romfs`) and the mods folder
//! beside them, where Longshot reads mods (its README, "Mods").

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Game {
    /// The game folder (`<Longshot>/v3.0.0-rc.3`).
    pub dir: PathBuf,
    /// The game's files.
    pub romfs: PathBuf,
    /// The mods folder (made when it isn't there).
    pub mods: PathBuf,
    /// From `longshot_version.txt` (`version=v3.0.0-rc.5`), when it's there.
    pub version: Option<String>,
}

/// Files beside `romfs` that mark Longshot's game folder.
const MARKERS: [&str; 4] = [
    "longshot.exe",
    "longshot",
    "longshot_version.txt",
    "bin/longshot.exe",
];

/// The game folder for `chosen`: the folder itself, the one it's in when it's the `romfs` or `mods` folder, or one
/// folder down (the launcher's folder holds the game in a version folder: `Longshot_OOT_Remake/v3.0.0-rc.3`). Of
/// several, the one updated last.
pub fn find(chosen: &Path) -> Result<Game, String> {
    let mut candidates = vec![chosen.to_owned()];
    let name = chosen
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase());
    if let (Some("romfs" | "mods"), Some(parent)) = (name.as_deref(), chosen.parent()) {
        candidates.push(parent.to_owned());
    }
    if let Ok(entries) = std::fs::read_dir(chosen) {
        candidates.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    }
    let mut games: Vec<Game> = candidates.iter().filter_map(|c| game_at(c)).collect();
    games.sort_by_key(|g| std::cmp::Reverse(updated(&g.dir)));
    games.into_iter().next().ok_or_else(|| {
        format!(
            "{} isn't a Longshot folder. Choose the folder Longshot is installed in: the one with \
             LongshotLauncher.exe, or the game folder in it with longshot.exe and romfs.",
            chosen.display()
        )
    })
}

fn game_at(dir: &Path) -> Option<Game> {
    let romfs = dir.join("romfs");
    if !modstudio_mods::is_game_romfs(&romfs) || !MARKERS.iter().any(|m| dir.join(m).is_file()) {
        return None;
    }
    Some(Game {
        dir: dir.to_owned(),
        mods: dir.join("mods"),
        version: version_of(dir),
        romfs,
    })
}

fn version_of(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("longshot_version.txt")).ok()?;
    text.lines()
        .find_map(|l| l.trim().strip_prefix("version="))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn updated(dir: &Path) -> Option<SystemTime> {
    ["longshot_version.txt", "romfs"].iter().find_map(|f| {
        std::fs::metadata(dir.join(f))
            .and_then(|m| m.modified())
            .ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up game folder: what the checks look at, no game files.
    fn fake_game(dir: &Path, version: &str) {
        std::fs::create_dir_all(dir.join("romfs/Model")).unwrap();
        std::fs::create_dir_all(dir.join("romfs/Pack")).unwrap();
        std::fs::write(dir.join("romfs/Pack/ZsDic.pack.zs"), b"x").unwrap();
        std::fs::write(dir.join("longshot.exe"), b"x").unwrap();
        std::fs::write(
            dir.join("longshot_version.txt"),
            format!("version={version}\nbuilder=x\n"),
        )
        .unwrap();
    }

    #[test]
    fn found_from_the_folders_a_user_may_pick() {
        let root = std::env::temp_dir().join(format!("longshot-mods-game-{}", std::process::id()));
        let launcher = root.join("Longshot_OOT_Remake");
        let game_dir = launcher.join("v3.0.0-rc.3");
        fake_game(&game_dir, "v3.0.0-rc.5");
        std::fs::create_dir_all(launcher.join("LongshotLauncher-data")).unwrap();

        for chosen in [
            &launcher,
            &game_dir,
            &game_dir.join("romfs"),
            &game_dir.join("mods"),
        ] {
            let game = find(chosen).unwrap();
            assert_eq!(game.dir, game_dir, "from {}", chosen.display());
            assert_eq!(game.mods, game_dir.join("mods"));
            assert_eq!(game.version.as_deref(), Some("v3.0.0-rc.5"));
        }
        // not a game: no romfs, or a romfs with nothing of Longshot's beside it
        assert!(find(&launcher.join("LongshotLauncher-data")).is_err());
        std::fs::remove_file(game_dir.join("longshot.exe")).unwrap();
        std::fs::remove_file(game_dir.join("longshot_version.txt")).unwrap();
        assert!(
            find(&launcher)
                .unwrap_err()
                .contains("isn't a Longshot folder")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
