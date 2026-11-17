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
2. Put each mod in the `mods` folder as its own folder, with the mod's `romfs` folder inside:
   `mods/Some Mod/romfs/...`. Mods kept as emulators keep them work too (`mods/contents/<title id>/Some Mod/romfs`).
3. The mod shows up in the list, and the size table is updated a moment later. Use the switch to turn a mod off
   (its folder gets a `-` in front of its name, which Longshot skips) or on again.
4. Restart the game: Longshot reads its mods when it starts.

When two mods change the same file, the one whose folder sorts last wins. Name folders `01-...`, `02-...` to set the
order.

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
