//! Installing mods from a zip or a folder the user drops (or picks): the mods in it are found whatever their
//! layout and put in the mods folder as Longshot reads them, `mods/<name>/romfs/...`.
//!
//! A mod is a folder holding `romfs` (or `exefs`, `cheats`), as Longshot finds them. In a download that folder may
//! be the top (`romfs/...`), one or more folders down (`Some Mod/romfs`, `Some Mod/Some Mod/romfs`), or in an
//! emulator's or Atmosphere's layout (`atmosphere/contents/<title id>/romfs`). Only those three folders are
//! installed (not read-me files or pictures beside them). Several mods in one download are usually versions to
//! choose from, so they're installed off. A mod already there is replaced only when the user says so.

use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use modstudio_mods::fsutil;
use modstudio_mods::mods::TABLE_MOD;
use serde::Serialize;

/// The folders of a mod that are installed (Longshot reads these).
const PARTS: [&str; 3] = ["romfs", "exefs", "cheats"];

/// Folder names that say nothing about a mod: emulators' and Atmosphere's layout (and title ids, see
/// [`is_title_id`]).
const LAYOUT: [&str; 3] = ["contents", "atmosphere", "mods"];

/// Where a download is put together before it's moved into the mods folder (a hidden folder in it, so the move is a
/// rename; Longshot, the mods list and the watcher leave hidden folders alone).
const STAGING: &str = ".modmanager-install";

/// A mod found in a download.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Found {
    /// Its folder in the download ('/'-separated, "" for the top).
    root: String,
    /// The name it's installed under.
    name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    /// Its folder in the mods folder.
    pub folder: String,
    pub enabled: bool,
    pub files: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub installed: Vec<Installed>,
    /// Mods already in the mods folder under the names the download's would take: nothing was installed, and
    /// installing again with `replace` replaces them.
    pub existing: Vec<String>,
    pub notes: Vec<String>,
}

/// A download: a zip, or a folder (with only some of its files, when the user dropped a mod's `romfs` itself).
enum Source {
    Zip(PathBuf),
    Folder { base: PathBuf, only: Option<String> },
}

fn is_title_id(name: &str) -> bool {
    name.len() == 16 && name.chars().all(|c| c.is_ascii_hexdigit())
}

fn is_part(name: &str) -> bool {
    PARTS.iter().any(|p| name.eq_ignore_ascii_case(p))
}

/// A folder name fit for the mods folder: on (no leading '-' or '_'), shown (no leading '.'), and allowed on
/// Windows; "Mod" when nothing is left, or for the size table's own name.
fn clean_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let kept = kept
        .trim_start_matches(['-', '_', '.', ' '])
        .trim_end_matches(['.', ' ']);
    if kept.is_empty() || kept.eq_ignore_ascii_case(TABLE_MOD) {
        "Mod".into()
    } else {
        kept.to_owned()
    }
}

/// The mods in a download, from its files' paths ('/'-separated): the folders holding `romfs`, `exefs` or
/// `cheats`, not ones inside another mod, each named after the nearest folder that says something (else
/// `fallback`: the zip's or folder's name). Names are made unique.
fn find(paths: &[String], fallback: &str) -> Vec<Found> {
    let mut roots: BTreeSet<String> = BTreeSet::new();
    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.iter().any(|p| p.starts_with('.') || *p == "__MACOSX") {
            continue;
        }
        // the first romfs/exefs/cheats from the top, with a file under it
        if let Some(i) = parts[..parts.len().saturating_sub(1)]
            .iter()
            .position(|p| is_part(p))
        {
            roots.insert(parts[..i].join("/"));
        }
    }
    // not mods inside a mod (Longshot doesn't look inside one)
    let roots: Vec<String> = roots
        .iter()
        .filter(|r| {
            !roots
                .iter()
                .any(|o| o != *r && (o.is_empty() || r.starts_with(&format!("{o}/"))))
        })
        .cloned()
        .collect();
    let mut names: BTreeSet<String> = BTreeSet::new();
    roots
        .into_iter()
        .map(|root| {
            let said = root
                .split('/')
                .rev()
                .find(|c| {
                    !c.is_empty()
                        && !is_title_id(c)
                        && !LAYOUT.iter().any(|l| c.eq_ignore_ascii_case(l))
                })
                .unwrap_or(fallback);
            let base = clean_name(said);
            let mut name = base.clone();
            let mut n = 2;
            while !names.insert(name.to_lowercase()) {
                name = format!("{base} ({n})");
                n += 1;
            }
            Found { root, name }
        })
        .collect()
}

/// `path` with '/' (a file's path relative to a download).
fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A download as messages name it: its file or folder name.
fn shown(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What `path` (dropped or picked) is: a zip, a folder, or a mod's own `romfs` (`exefs`, `cheats`) folder.
fn source_of(path: &Path) -> Result<(Source, String), String> {
    if path.is_dir() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_part(&name)
            && let Some(parent) = path.parent()
        {
            let fallback = parent
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let only = Some(name.to_lowercase());
            return Ok((
                Source::Folder {
                    base: parent.to_owned(),
                    only,
                },
                fallback,
            ));
        }
        return Ok((
            Source::Folder {
                base: path.to_owned(),
                only: None,
            },
            name,
        ));
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "zip" => Ok((Source::Zip(path.to_owned()), file_stem(path))),
        "7z" | "rar" => Err(format!(
            "{}: a .{ext} archive can't be opened here. Extract it, then drop the folder it makes.",
            shown(path)
        )),
        _ => Err(format!(
            "{} isn't a mod: drop a zip, or a mod's folder",
            shown(path)
        )),
    }
}

/// The files of a download, as '/'-separated paths in it.
fn paths_of(source: &Source) -> Result<Vec<String>, String> {
    match source {
        Source::Folder { base, only } => {
            let all = fsutil::files_under(base)
                .iter()
                .map(|p| slashed(p))
                .collect();
            Ok(match only {
                None => all,
                Some(part) => all
                    .into_iter()
                    .filter(|p: &String| {
                        p.split('/')
                            .next()
                            .is_some_and(|f| f.eq_ignore_ascii_case(part))
                    })
                    .collect(),
            })
        }
        Source::Zip(zip) => {
            let mut archive = open_zip(zip)?;
            let mut out = Vec::new();
            for i in 0..archive.len() {
                let entry = archive
                    .by_index(i)
                    .map_err(|e| format!("{}: {e}", zip.display()))?;
                if entry.is_file()
                    && let Some(name) = entry.enclosed_name()
                {
                    out.push(slashed(&name));
                }
            }
            Ok(out)
        }
    }
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<File>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", path.display()))
}

/// `path` in the download, as the installed mod has it: its part's folder in lower case and the rest as it is,
/// when it's in `found`'s romfs, exefs or cheats.
fn in_mod(found: &Found, path: &str) -> Option<String> {
    let rest = if found.root.is_empty() {
        path
    } else {
        path.strip_prefix(&found.root)?.strip_prefix('/')?
    };
    let (part, inside) = rest.split_once('/')?;
    is_part(part).then(|| format!("{}/{inside}", part.to_lowercase()))
}

/// The mods folder's entry for `name`, either way it is (on, or off with '-' or '_').
fn existing(mods: &Path, name: &str) -> Vec<PathBuf> {
    ["", "-", "_"]
        .iter()
        .map(|p| mods.join(format!("{p}{name}")))
        .filter(|p| p.exists())
        .collect()
}

/// Installs the mods in `paths` (zips and folders) into `mods`. When a mod of the same name is there, nothing is
/// installed and [`Report::existing`] names them, unless `replace`: then they're replaced, keeping whether they
/// were on.
pub fn install(mods: &Path, paths: &[PathBuf], replace: bool) -> Result<Report, String> {
    let mut report = Report::default();
    let mut plans = Vec::new();
    for path in paths {
        let (source, fallback) = source_of(path)?;
        let files = paths_of(&source)?;
        let found = find(&files, &fallback);
        if found.is_empty() {
            return Err(format!(
                "No mod in {}: a mod has a romfs folder (or exefs, cheats) with its files",
                shown(path)
            ));
        }
        plans.push((path, source, files, found));
    }
    // names taken by earlier downloads in this drop, then by mods already installed
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (_, _, _, found) in &mut plans {
        for f in found.iter_mut() {
            let base = f.name.clone();
            let mut n = 2;
            while !taken.insert(f.name.to_lowercase()) {
                f.name = format!("{base} ({n})");
                n += 1;
            }
            if !existing(mods, &f.name).is_empty() {
                report.existing.push(f.name.clone());
            }
        }
    }
    if !report.existing.is_empty() && !replace {
        return Ok(report);
    }
    report.existing.clear();

    let staging = mods.join(STAGING);
    fsutil::remove_dir(&staging).map_err(|e| e.0)?;
    let result = install_planned(mods, &staging, &plans, &mut report);
    let _ = fsutil::remove_dir(&staging);
    result.map(|()| report)
}

/// A download: where it is, what it is, its files, and the mods found in it.
type Plan<'a> = (&'a PathBuf, Source, Vec<String>, Vec<Found>);

/// Each mod written into `staging`, then moved into `mods` in place of one of the same name.
fn install_planned(
    mods: &Path,
    staging: &Path,
    plans: &[Plan],
    report: &mut Report,
) -> Result<(), String> {
    for (path, source, files, found) in plans {
        let several = found.len() > 1;
        for f in found {
            let built = staging.join(&f.name);
            let count = write_mod(source, files, f, &built)?;
            let old = existing(mods, &f.name);
            let was_off = old.iter().any(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(['-', '_']))
            });
            for p in &old {
                fsutil::remove_dir(p).map_err(|e| e.0)?;
            }
            let enabled = if old.is_empty() { !several } else { !was_off };
            let folder = if enabled {
                f.name.clone()
            } else {
                format!("-{}", f.name)
            };
            std::fs::rename(&built, mods.join(&folder))
                .map_err(|e| format!("moving {} into the mods folder: {e}", f.name))?;
            report.installed.push(Installed {
                folder,
                enabled,
                files: count,
            });
        }
        if several {
            report.notes.push(format!(
                "{} holds {} mods, installed off: they're often versions to choose from, so turn on the \
                 one you want",
                shown(path),
                found.len()
            ));
        }
    }
    Ok(())
}

/// Writes `found`'s romfs, exefs and cheats from `source` into `dir`. Returns how many files.
fn write_mod(
    source: &Source,
    files: &[String],
    found: &Found,
    dir: &Path,
) -> Result<usize, String> {
    let write_at = |rel: &str| -> Result<PathBuf, String> {
        let to = dir.join(rel);
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        Ok(to)
    };
    let mut count = 0;
    match source {
        Source::Folder { base, .. } => {
            for path in files {
                if let Some(rel) = in_mod(found, path) {
                    let to = write_at(&rel)?;
                    std::fs::copy(base.join(path), &to)
                        .map_err(|e| format!("{}: {e}", to.display()))?;
                    count += 1;
                }
            }
        }
        Source::Zip(zip) => {
            let mut archive = open_zip(zip)?;
            for i in 0..archive.len() {
                let mut entry = archive
                    .by_index(i)
                    .map_err(|e| format!("{}: {e}", zip.display()))?;
                let Some(name) = entry.enclosed_name().filter(|_| entry.is_file()) else {
                    continue;
                };
                if let Some(rel) = in_mod(found, &slashed(&name)) {
                    let to = write_at(&rel)?;
                    let mut out =
                        File::create(&to).map_err(|e| format!("{}: {e}", to.display()))?;
                    std::io::copy(&mut entry, &mut out)
                        .map_err(|e| format!("{}: {e}", to.display()))?;
                    count += 1;
                }
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn names(paths: &[&str], fallback: &str) -> Vec<(String, String)> {
        let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        find(&paths, fallback)
            .into_iter()
            .map(|f| (f.root, f.name))
            .collect()
    }

    fn pair(root: &str, name: &str) -> (String, String) {
        (root.to_owned(), name.to_owned())
    }

    #[test]
    fn mods_found_whatever_the_layout() {
        // the usual: the mod's folder at the top
        assert_eq!(
            names(&["My Mod/romfs/Tex/a.zs", "My Mod/readme.txt"], "dl"),
            [pair("My Mod", "My Mod")]
        );
        // romfs at the top: the zip's name
        assert_eq!(
            names(&["romfs/Tex/a.zs"], "Cool Mod v2"),
            [pair("", "Cool Mod v2")]
        );
        // a folder too many
        assert_eq!(names(&["X/X/romfs/a.zs"], "dl"), [pair("X/X", "X")]);
        // Atmosphere's layout, in a folder or not
        let atmo = "atmosphere/contents/0100F2C0115B6000/romfs/a.zs";
        assert_eq!(
            names(&[atmo], "Hat"),
            [pair("atmosphere/contents/0100F2C0115B6000", "Hat")]
        );
        assert_eq!(
            names(&[&format!("Big Hat/{atmo}")], "dl"),
            [pair(
                "Big Hat/atmosphere/contents/0100F2C0115B6000",
                "Big Hat"
            )]
        );
        // versions to choose from
        assert_eq!(
            names(
                &[
                    "Pack/Red/romfs/a.zs",
                    "Pack/Blue/romfs/a.zs",
                    "Pack/Blue/cheats/x.txt"
                ],
                "dl"
            ),
            [pair("Pack/Blue", "Blue"), pair("Pack/Red", "Red")]
        );
        // not a mod inside a mod, nor macOS's extra folder, nor a folder named romfs with nothing in it
        assert_eq!(
            names(
                &[
                    "M/romfs/a.zs",
                    "M/old/romfs/b.zs",
                    "__MACOSX/M/romfs/._a.zs",
                    "N/romfs"
                ],
                "dl"
            ),
            [pair("M", "M")]
        );
        // names fit for the mods folder, and unique
        assert_eq!(
            names(&["-Off?/romfs/a", "a/Same/romfs/x", "b/same/romfs/x"], "dl"),
            [
                pair("-Off?", "Off_"),
                pair("a/Same", "Same"),
                pair("b/same", "same (2)")
            ]
        );
        assert!(names(&["readme.txt", "pics/a.png"], "dl").is_empty());
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("longshot-mods-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_of(path: &Path, files: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, text) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn a_zip_installed_and_replaced() {
        let dir = temp("zip");
        let mods = dir.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let zip = dir.join("download.zip");
        zip_of(
            &zip,
            &[
                ("My Mod/ROMFS/Tex/a.zs", "1"),
                ("My Mod/readme.txt", "hi"),
                ("../evil.txt", "no"),
                ("/abs.txt", "no"),
            ],
        );
        let report = install(&mods, std::slice::from_ref(&zip), false).unwrap();
        assert_eq!(report.installed.len(), 1);
        assert!(report.installed[0].enabled && report.installed[0].files == 1);
        assert_eq!(
            std::fs::read_to_string(mods.join("My Mod/romfs/Tex/a.zs")).unwrap(),
            "1"
        );
        assert!(!mods.join("My Mod/readme.txt").exists());
        assert!(!dir.join("evil.txt").exists() && !mods.join(STAGING).exists());

        // again: it's there, so nothing happens until replace (the mod was turned off: it stays off)
        std::fs::rename(mods.join("My Mod"), mods.join("-My Mod")).unwrap();
        zip_of(&zip, &[("My Mod/romfs/Tex/a.zs", "2")]);
        let report = install(&mods, std::slice::from_ref(&zip), false).unwrap();
        assert_eq!(report.existing, ["My Mod"]);
        assert!(report.installed.is_empty());
        let report = install(&mods, std::slice::from_ref(&zip), true).unwrap();
        assert_eq!(report.installed[0].folder, "-My Mod");
        assert_eq!(
            std::fs::read_to_string(mods.join("-My Mod/romfs/Tex/a.zs")).unwrap(),
            "2"
        );
        assert!(!mods.join("My Mod").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folders_installed_versions_off() {
        let dir = temp("folder");
        let mods = dir.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        for f in [
            "Pack/Red/romfs/a.zs",
            "Pack/Blue/romfs/a.zs",
            "Solo/romfs/b.zs",
            "Solo/notes.txt",
        ] {
            let p = dir.join("dl").join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"x").unwrap();
        }
        // several mods: all off
        let report = install(&mods, &[dir.join("dl/Pack")], false).unwrap();
        let folders: Vec<&str> = report.installed.iter().map(|i| i.folder.as_str()).collect();
        assert_eq!(folders, ["-Blue", "-Red"]);
        assert_eq!(report.notes.len(), 1);
        // a mod's romfs dropped by itself: named after the folder it's in, only it copied
        let report = install(&mods, &[dir.join("dl/Solo/romfs")], false).unwrap();
        assert_eq!(report.installed[0].folder, "Solo");
        assert!(mods.join("Solo/romfs/b.zs").is_file() && !mods.join("Solo/notes.txt").exists());
        // the download is left as it was
        assert!(dir.join("dl/Solo/romfs/b.zs").is_file());
        // not mods
        assert!(install(&mods, &[dir.join("dl/Solo/notes.txt")], false).is_err());
        assert!(
            install(&mods, &[dir.join("x.7z")], false)
                .unwrap_err()
                .contains("Extract it")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
