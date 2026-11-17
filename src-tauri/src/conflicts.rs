//! Mods that change the same files. Longshot uses the file of the mod whose path in the mods folder sorts last
//! (ignoring case), so the other mod's version of it is never seen. Size tables aren't counted: the one for all
//! mods replaces them.

use std::collections::BTreeMap;
use std::path::Path;

use modstudio_mods::{fsutil, mods};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// The mod whose files are used (it sorts later).
    pub used: String,
    /// The mod whose files aren't.
    pub hidden: String,
    /// The files both have (paths in the romfs, as `used` spells them), sorted.
    pub files: Vec<String>,
}

/// The conflicts between the mods that are on (folders as [`mods::list`] gives them), by the mod that's used,
/// then the one hidden.
pub fn conflicts(mods_dir: &Path) -> Result<Vec<Conflict>, String> {
    let on: Vec<_> = mods::list(mods_dir)
        .map_err(|e| e.0)?
        .into_iter()
        .filter(|m| m.enabled)
        .collect();
    // a file's path in lower case -> its spelling in the last mod, and the mods that have it (in order)
    let mut by_path: BTreeMap<String, (String, Vec<usize>)> = BTreeMap::new();
    for (i, m) in on.iter().enumerate() {
        for rel in fsutil::files_under(&mods_dir.join(&m.folder).join("romfs")) {
            if fsutil::is_size_table(&rel) {
                continue;
            }
            let spelled = rel.to_string_lossy().replace('\\', "/");
            let entry = by_path.entry(spelled.to_lowercase()).or_default();
            entry.0 = spelled;
            entry.1.push(i);
        }
    }
    let mut pairs: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
    for (spelled, holders) in by_path.into_values() {
        if let Some((&used, hidden)) = holders.split_last() {
            for &h in hidden {
                pairs.entry((used, h)).or_default().push(spelled.clone());
            }
        }
    }
    Ok(pairs
        .into_iter()
        .map(|((used, hidden), files)| Conflict {
            used: on[used].folder.clone(),
            hidden: on[hidden].folder.clone(),
            files,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_mod_wins_its_files() {
        let dir =
            std::env::temp_dir().join(format!("longshot-mods-conflicts-{}", std::process::id()));
        for f in [
            "A/romfs/Tex/x.zs",
            "A/romfs/Tex/only_a.zs",
            "B/romfs/tex/X.zs",
            "B/romfs/Model/m.zs",
            "C/romfs/Model/m.zs",
            "-D/romfs/Tex/x.zs",
            "A/romfs/System/Resource/R.rsizetable.zs",
            "C/romfs/System/Resource/R.rsizetable.zs",
        ] {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        let found = conflicts(&dir).unwrap();
        let conflict = |used: &str, hidden: &str, file: &str| Conflict {
            used: used.into(),
            hidden: hidden.into(),
            files: vec![file.into()],
        };
        // case doesn't matter; mods that are off and size tables don't count
        assert_eq!(
            found,
            [
                conflict("B", "A", "tex/X.zs"),
                conflict("C", "B", "Model/m.zs")
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
