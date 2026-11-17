# Mod Manager for Longshot

Point it at your Longshot folder and put mods in the `mods` folder: it lists them, turns them on and off, and keeps
**one resource size table for every mod that is on**, rebuilt by itself whenever the mods folder changes.

It's a third-party tool, not part of Longshot. It never changes the game's own files, only what's in `mods`.

## Why the size table matters

The game sets aside memory for each file by its entry in a resource size table. A mod file larger than the game's
own (a new model, HD textures, longer parameter files) doesn't load unless the table gives it room: a texture shows
up flat or grey, or the game crashes or waits at a loading screen.

Longshot uses **only one table**: the one from the mod whose folder sorts last. So when two mods each ship their own
table, only the last one's files get their room, and the other mods break. This manager works out one table for all
the mods that are on, from the game's own table and each mod's files, and installs it as its own mod,
`zzz-ResourceSizeTable`, which sorts after the others. A mod's own table isn't lost: its larger entries are kept.

## Using it

1. Run **Mod Manager for Longshot** and choose your Longshot folder: the one with `LongshotLauncher.exe`, or the
   game folder inside it (with `longshot.exe` and `romfs`). A `mods` folder is made beside the game if there isn't
   one yet.
2. Drop a mod's zip or folder onto the window (or use **Add zip…** / **Add folder…**). The mod is found whatever
   its layout (`romfs` at the top of the zip, a folder or two down, or kept as emulators and Atmosphere keep them)
   and installed as `mods/<name>/romfs`. Only the mod's `romfs`, `exefs` and `cheats` folders are copied. A zip
   holding several mods (usually versions to choose from) installs them all turned off: turn on the one you want.
   Installing a mod that's already there asks before replacing it, and keeps it on or off as it was.
   `.7z` and `.rar` archives can't be opened yet: extract them and drop the folder.
3. Or put the mod in the `mods` folder yourself, as its own folder with its `romfs` folder inside:
   `mods/Some Mod/romfs/...`. Mods kept as emulators keep them work too (`mods/contents/<title id>/Some Mod/romfs`).
4. The mod shows up in the list, and the size table is updated a moment later. Use the switch to turn a mod off
   (its folder gets a `-` in front of its name, which Longshot skips) or on again.
5. Restart the game: Longshot reads its mods when it starts. While Longshot is running the manager says so, and
   when the game closes it makes the table again.

When two mods change the same file, the one whose folder sorts last wins. The **Conflicts** list shows which mod is
used over which, for which files, and marks the mods with files that aren't used. Name folders `01-...`, `02-...`
to set the order, or turn one off.

The manager remembers the folder in `settings.json` in its settings folder (`%APPDATA%\io.github.moosemodder.longshotmods`
on Windows).

## With Hookshot Mod Studio

[Hookshot Mod Studio](https://github.com/moose-modder/hookshot-mod-studio) installs the mods it builds into the same
`mods` folder and builds the same table: both use the same code (`modstudio-mods`). The table's `table-stamp.json`
names the app that built it and the version of its rules, and an app with older rules leaves a newer table alone and
says which app to update.

## Building

You need Rust (the version in `rust-toolchain.toml`; rustup picks it up) and Node.js.

```
npm ci
npm run tauri dev       # run it, with the page reloading as you edit
npm run tauri build     # target/release/mod-manager-for-longshot.exe and the installer
cargo test --workspace
```

The mods folder and the table come from `modstudio-mods` in the Mod Studio repository, pinned in `Cargo.lock`.
`cargo update -p modstudio-mods` takes its newest `main`.

For checking the page with DevTools: `MODMANAGER_WEBVIEW_ARGS=--remote-debugging-port=9334` (Windows), and
`MODMANAGER_SETTINGS=<file>` to use other settings than your own.

## License

GPL-3.0-or-later (`LICENSE`), as Hookshot Mod Studio, whose code it uses.
