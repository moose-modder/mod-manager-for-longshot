//! The game the mods are for. Two folders matter:
//! - the game's files (`romfs`), which the size table is worked out from;
//! - the mods folder.
//!
//! Longshot and Hookshot keep both in their game folder (`<game folder>/romfs` and `<game folder>/mods`; each
//! README, "Mods"). A folder the user picks is looked at for that layout. Anything else (an emulator's mods folder,
//! or game files dumped somewhere) is chosen as the two folders, and nothing about it is required but the game's
//! files.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Game {
    /// Longshot's or Hookshot's game folder (`<Longshot>/v3.0.0-rc.3`). None when the two folders were chosen on
    /// their own.
    pub dir: Option<PathBuf>,
    /// The game's files.
    pub romfs: PathBuf,
    /// The mods folder (made when it isn't there).
    pub mods: PathBuf,
    /// "Longshot" or "Hookshot", from the files beside `romfs`.
    pub program: Option<&'static str>,
    /// From the program's version file (`version=v3.0.0-rc.5`), when it's there.
    pub version: Option<String>,
}

/// A program that keeps the game's files and its mods in one game folder.
pub struct Program {
    pub name: &'static str,
    /// Files beside `romfs` that mark its game folder.
    markers: [&'static str; 4],
    version_file: &'static str,
}

pub const PROGRAMS: [Program; 2] = [
    Program {
        name: "Longshot",
        markers: [
            "longshot.exe",
            "longshot",
            "longshot_version.txt",
            "bin/longshot.exe",
        ],
        version_file: "longshot_version.txt",
    },
    Program {
        name: "Hookshot",
        markers: [
            "hookshot.exe",
            "hookshot",
            "hookshot_version.txt",
            "bin/hookshot.exe",
        ],
        version_file: "hookshot_version.txt",
    },
];

/// The game folder for `chosen`. It may be:
/// - the folder itself;
/// - the one it's in, when it's the `romfs` or `mods` folder;
/// - one folder down (Longshot's launcher folder holds the game in a version folder:
///   `Longshot_OOT_Remake/v3.0.0-rc.3`).
///
/// Of several, the one updated last.
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
    games.sort_by_key(|g| std::cmp::Reverse(g.dir.as_deref().and_then(updated)));
    games.into_iter().next().ok_or_else(|| {
        format!(
            "{} isn't a Longshot or Hookshot folder. Choose the folder Longshot or Hookshot is installed in, or \
             choose the game's files and the mods folder yourself.",
            chosen.display()
        )
    })
}

/// The game's files and a mods folder chosen on their own. `romfs` may be the game's files or the folder they're in.
pub fn chosen(romfs: &Path, mods: &Path) -> Result<Game, String> {
    let romfs = [romfs.to_owned(), romfs.join("romfs")]
        .into_iter()
        .find(|d| modstudio_mods::is_game_romfs(d))
        .ok_or_else(|| {
            format!(
                "{} isn't the game's files. Choose the romfs folder (the one with Model and Pack in it), or the \
                 folder it's in.",
                romfs.display()
            )
        })?;
    if mods.starts_with(&romfs) {
        return Err(format!(
            "The mods folder can't be inside the game's files ({}).",
            romfs.display()
        ));
    }
    Ok(Game {
        dir: None,
        romfs,
        mods: mods.to_owned(),
        program: None,
        version: None,
    })
}

fn game_at(dir: &Path) -> Option<Game> {
    let romfs = dir.join("romfs");
    if !modstudio_mods::is_game_romfs(&romfs) {
        return None;
    }
    let program = PROGRAMS
        .iter()
        .find(|p| p.markers.iter().any(|m| dir.join(m).is_file()))?;
    Some(Game {
        dir: Some(dir.to_owned()),
        mods: dir.join("mods"),
        version: version_of(&dir.join(program.version_file)),
        program: Some(program.name),
        romfs,
    })
}

fn version_of(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    text.lines()
        .find_map(|l| l.trim().strip_prefix("version="))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn updated(dir: &Path) -> Option<SystemTime> {
    PROGRAMS
        .iter()
        .map(|p| p.version_file)
        .chain(["romfs"])
        .find_map(|f| {
            std::fs::metadata(dir.join(f))
                .and_then(|m| m.modified())
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up game folder: what the checks look at, no game files.
    fn fake_game(dir: &Path, program: &str, version: &str) {
        fake_romfs(&dir.join("romfs"));
        std::fs::write(dir.join(format!("{program}.exe")), b"x").unwrap();
        std::fs::write(
            dir.join(format!("{program}_version.txt")),
            format!("version={version}\nbuilder=x\n"),
        )
        .unwrap();
    }

    fn fake_romfs(dir: &Path) {
        std::fs::create_dir_all(dir.join("Model")).unwrap();
        std::fs::create_dir_all(dir.join("Pack")).unwrap();
        std::fs::write(dir.join("Pack/ZsDic.pack.zs"), b"x").unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("longshot-mods-game-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn longshot_found_from_the_folders_a_user_may_pick() {
        let root = scratch("longshot");
        let launcher = root.join("Longshot_OOT_Remake");
        let game_dir = launcher.join("v3.0.0-rc.3");
        fake_game(&game_dir, "longshot", "v3.0.0-rc.5");
        std::fs::create_dir_all(launcher.join("LongshotLauncher-data")).unwrap();

        for chosen in [
            &launcher,
            &game_dir,
            &game_dir.join("romfs"),
            &game_dir.join("mods"),
        ] {
            let game = find(chosen).unwrap();
            assert_eq!(
                game.dir.as_ref(),
                Some(&game_dir),
                "from {}",
                chosen.display()
            );
            assert_eq!(game.mods, game_dir.join("mods"));
            assert_eq!(game.program, Some("Longshot"));
            assert_eq!(game.version.as_deref(), Some("v3.0.0-rc.5"));
        }
        // not a game: no romfs, or a romfs with neither program's files beside it
        assert!(find(&launcher.join("LongshotLauncher-data")).is_err());
        std::fs::remove_file(game_dir.join("longshot.exe")).unwrap();
        std::fs::remove_file(game_dir.join("longshot_version.txt")).unwrap();
        assert!(
            find(&launcher)
                .unwrap_err()
                .contains("isn't a Longshot or Hookshot folder")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn hookshot_found_in_its_folder() {
        let root = scratch("hookshot");
        let game_dir = root.join("OOT_Remake");
        fake_game(&game_dir, "hookshot", "v2.1.0");
        for chosen in [&game_dir, &game_dir.join("romfs")] {
            let game = find(chosen).unwrap();
            assert_eq!(game.dir.as_ref(), Some(&game_dir));
            assert_eq!(game.mods, game_dir.join("mods"));
            assert_eq!(game.program, Some("Hookshot"));
            assert_eq!(game.version.as_deref(), Some("v2.1.0"));
        }
        // its program only in bin/, and no version file
        std::fs::remove_file(game_dir.join("hookshot.exe")).unwrap();
        std::fs::remove_file(game_dir.join("hookshot_version.txt")).unwrap();
        std::fs::create_dir_all(game_dir.join("bin")).unwrap();
        std::fs::write(game_dir.join("bin/hookshot.exe"), b"x").unwrap();
        let game = find(&game_dir).unwrap();
        assert_eq!((game.program, game.version), (Some("Hookshot"), None));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_two_folders_chosen_on_their_own() {
        let root = scratch("chosen");
        let dump = root.join("dump");
        fake_romfs(&dump.join("romfs"));
        let mods = root.join("emulator/load/0123456789abcdef");
        for picked in [dump.clone(), dump.join("romfs")] {
            let game = chosen(&picked, &mods).unwrap();
            assert_eq!(game.romfs, dump.join("romfs"));
            assert_eq!(game.mods, mods);
            assert_eq!((game.dir, game.program, game.version), (None, None, None));
        }
        assert!(
            chosen(&root, &mods)
                .unwrap_err()
                .contains("isn't the game's files")
        );
        assert!(
            chosen(&dump, &dump.join("romfs/mods"))
                .unwrap_err()
                .contains("can't be inside the game's files")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
