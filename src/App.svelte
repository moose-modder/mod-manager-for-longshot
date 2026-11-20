<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { ask, open } from "@tauri-apps/plugin-dialog";

  /** dir and program: Longshot's or Hookshot's game folder; none when the game's files and the mods folder were
   * chosen on their own (an emulator) */
  type Game = {
    dir: string | null;
    romfs: string;
    mods: string;
    program: string | null;
    version: string | null;
  };
  type TableState = "idle" | "waiting" | "building" | "ready" | "none" | "error";
  type TableStatus = {
    state: TableState;
    message: string;
    log: string[];
    warnings: string[];
    finished: number | null;
  };
  type StateView = {
    game: Game | null;
    problem: string | null;
    status: TableStatus;
    settings_path: string;
    made_by: string;
    /** Longshot or Hookshot, when one is running */
    running: string | null;
  };
  type Mod = {
    name: string;
    folder: string;
    enabled: boolean;
    files: number;
    models: string[];
    textures: number;
    animations: number;
    own_size_table: boolean;
  };
  type Conflict = { used: string; hidden: string; files: string[] };
  type ModsView = {
    mods: Mod[];
    table: string | null;
    stamp: { rules: number; made_by: string } | null;
    conflicts: Conflict[];
  };
  type Report = {
    installed: { folder: string; enabled: boolean; files: number }[];
    existing: string[];
    notes: string[];
  };

  let view = $state<StateView | null>(null);
  let mods = $state<ModsView | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string[] | null>(null);
  let toggling = $state<string | null>(null);
  let choosing = $state(false);
  /** the folders being chosen again (the welcome card shown over the mods) */
  let changing = $state(false);
  let installing = $state(false);
  let dragging = $state(false);
  let showLog = $state(false);
  let openConflicts = $state<Record<string, boolean>>({});

  const status = $derived(view?.status);
  /** the program running, when it's the one whose mods these are (or the folders were chosen on their own) */
  const running = $derived(
    view?.running && (!view.game?.program || view.game.program === view.running) ? view.running : null,
  );
  const onCount = $derived(mods ? mods.mods.filter((m) => m.enabled).length : 0);
  /** by name, so a mod stays in its place when it's turned off (its folder gets a '-'); for the mods that are on,
   * that's the game's order */
  const byName = $derived(
    [...(mods?.mods ?? [])].sort((a, b) => {
      const [x, y] = [a.name.toLowerCase(), b.name.toLowerCase()];
      return x < y ? -1 : x > y ? 1 : 0;
    }),
  );
  /** a mod's files that another mod's replace, by its folder */
  const hiddenFiles = $derived.by(() => {
    const out: Record<string, number> = {};
    for (const c of mods?.conflicts ?? []) out[c.hidden] = (out[c.hidden] ?? 0) + c.files.length;
    return out;
  });

  const HEADLINES: Record<TableState, string> = {
    idle: "Size table",
    waiting: "Changes found",
    building: "Updating the size table…",
    ready: "Size table up to date",
    none: "No size table needed",
    error: "The size table couldn't be updated",
  };

  async function load() {
    try {
      view = await invoke<StateView>("get_state");
      await loadMods();
    } catch (e) {
      error = String(e);
    }
  }

  async function loadMods() {
    if (!view?.game) return;
    try {
      mods = await invoke<ModsView>("list_mods");
    } catch (e) {
      error = String(e);
    }
  }

  async function chooseFolder() {
    const dir = await open({ directory: true, title: "Choose your Longshot or Hookshot folder" });
    if (typeof dir !== "string") return;
    await use("choose_game_folder", { dir });
  }

  /** The game's files and the mods folder, chosen on their own (an emulator, game files kept elsewhere). */
  async function chooseFolders() {
    const romfs = await open({ directory: true, title: "Choose the game's files (the romfs folder)" });
    if (typeof romfs !== "string") return;
    const mods = await open({ directory: true, title: "Now choose the mods folder" });
    if (typeof mods !== "string") return;
    await use("choose_folders", { romfs, mods });
  }

  async function use(command: string, args: Record<string, string>) {
    error = null;
    choosing = true;
    try {
      view = await invoke<StateView>(command, args);
      changing = false;
      mods = null;
      await loadMods();
    } catch (e) {
      error = String(e);
    } finally {
      choosing = false;
    }
  }

  async function toggle(m: Mod, input: HTMLInputElement) {
    error = null;
    toggling = m.folder;
    try {
      await invoke("set_mod_enabled", { folder: m.folder, enabled: !m.enabled });
    } catch (e) {
      error = String(e);
      input.checked = m.enabled;
    } finally {
      toggling = null;
      await loadMods();
    }
  }

  /** Installs the zips and folders at `paths`, asking first when that would replace mods already there. */
  async function install(paths: string[]) {
    if (!view?.game || !paths.length || installing) return;
    error = null;
    notice = null;
    installing = true;
    try {
      let report = await invoke<Report>("install_mods", { paths, replace: false });
      if (report.existing.length) {
        const names = report.existing.map((n) => `"${n}"`).join(", ");
        const one = report.existing.length === 1;
        const yes = await ask(
          `${names} ${one ? "is" : "are"} already installed. Replace ${one ? "it" : "them"} with the new ` +
            `${one ? "one" : "ones"}? The installed ${one ? "copy is" : "copies are"} deleted.`,
          { title: "Replace mods?", kind: "warning", okLabel: "Replace", cancelLabel: "Keep what's there" },
        );
        if (!yes) return;
        report = await invoke<Report>("install_mods", { paths, replace: true });
      }
      notice = [
        ...report.installed.map(
          (i) =>
            `Installed ${leaf(i.folder.replace(/^-/, ""))} (${plural(i.files, "file")})` +
            (i.enabled ? "" : ", off"),
        ),
        ...report.notes,
      ];
      await loadMods();
    } catch (e) {
      error = String(e);
    } finally {
      installing = false;
    }
  }

  async function addZips() {
    const picked = await open({
      multiple: true,
      title: "Add mods from zips",
      filters: [{ name: "Zip archives", extensions: ["zip"] }],
    });
    if (picked) await install(Array.isArray(picked) ? picked : [picked]);
  }

  async function addFolder() {
    const picked = await open({ directory: true, title: "Add a mod's folder" });
    if (typeof picked === "string") await install([picked]);
  }

  function rebuild() {
    invoke("rebuild_now").catch((e) => (error = String(e)));
  }

  function openFolder(which: "mods" | "game" | "romfs") {
    invoke("open_folder", { which }).catch((e) => (error = String(e)));
  }

  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

  function details(m: Mod): string {
    const parts = [plural(m.files, "file")];
    if (m.models.length) parts.push(plural(m.models.length, "model"));
    if (m.textures) parts.push(plural(m.textures, "texture"));
    if (m.animations) parts.push(plural(m.animations, "animation"));
    return parts.join(" · ");
  }

  const leaf = (path: string) => path.split("/").pop() ?? path;
  const parent = (path: string) => path.split("/").slice(0, -1).join("/");
  const time = (secs: number) =>
    new Date(secs * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  const SHOWN = 200;

  onMount(() => {
    load();
    const stops = [
      listen<TableStatus>("table", (e) => {
        if (view) view.status = e.payload;
        loadMods();
      }),
      listen<string | null>("game", (e) => {
        if (view) view.running = e.payload;
      }),
      getCurrentWebview().onDragDropEvent((e) => {
        if (e.payload.type === "enter" || e.payload.type === "over") dragging = !!view?.game && !changing;
        else if (e.payload.type === "leave") dragging = false;
        else if (e.payload.type === "drop") {
          dragging = false;
          if (!changing) install(e.payload.paths);
        }
      }),
    ];
    return () => {
      for (const stop of stops) stop.then((f) => f());
    };
  });
</script>

<main>
  <header>
    <div class="grow">
      <h1>OoT Remake Mod Manager</h1>
      {#if view?.game?.dir}
        <p class="sub">
          {#if view.game.program}{view.game.program}{#if view.game.version}{" "}{view.game.version}{/if}{" · "}{/if}<button
            class="link"
            onclick={() => openFolder("game")}
            title="Open the game folder">{view.game.dir}</button
          >
        </p>
      {:else if view?.game}
        <p class="sub">
          Game files: <button class="link" onclick={() => openFolder("romfs")} title="Open the game's files"
            >{view.game.romfs}</button
          >
        </p>
        <p class="sub">
          Mods: <button class="link" onclick={() => openFolder("mods")} title="Open the mods folder"
            >{view.game.mods}</button
          >
        </p>
      {/if}
    </div>
    {#if view?.game && !changing}
      <button onclick={() => (changing = true)} disabled={choosing}>Change folders</button>
    {/if}
  </header>

  {#if running}
    <div class="banner info" role="status">
      <span class="grow">
        <strong>{running} is running.</strong> It reads its mods when it starts, so changes you make now show after you
        restart the game.
      </span>
    </div>
  {/if}

  {#if error}
    <div class="banner" role="alert">
      <span class="grow">{error}</span>
      <button class="link" onclick={() => (error = null)}>Dismiss</button>
    </div>
  {/if}

  {#if installing}
    <div class="banner info" role="status"><span class="grow">Installing…</span></div>
  {:else if notice}
    <div class="banner ok" role="status">
      <span class="grow">{#each notice as line}<span class="line">{line}</span>{/each}</span>
      <button class="link" onclick={() => (notice = null)}>Dismiss</button>
    </div>
  {/if}

  {#if view && (!view.game || changing)}
    <section class="card welcome">
      <h2>Where's the game?</h2>
      <p>
        Choose the folder Longshot or Hookshot is installed in: for Longshot, the one with
        <code>LongshotLauncher.exe</code> or the game folder inside it; for Hookshot, the one with
        <code>hookshot.exe</code>. A <code>mods</code> folder is made beside the game if there isn't one yet.
      </p>
      {#if view.problem && !view.game}<p class="note warn">{view.problem}</p>{/if}
      <button class="primary" onclick={chooseFolder} disabled={choosing}>
        {choosing ? "Looking…" : "Choose your Longshot or Hookshot folder"}
      </button>
      <p class="other">
        Playing on an emulator, or keeping the game's files somewhere else? Choose the game's files (the
        <code>romfs</code> folder) and the mods folder yourself. For an emulator, choose the folder it keeps this
        game's mods in (usually named after the game's title ID), so the size table goes beside them.
      </p>
      <div class="row wrap">
        <button onclick={chooseFolders} disabled={choosing}>Choose the game's files and mods folder…</button>
        {#if changing}<button class="link" onclick={() => (changing = false)}>Cancel</button>{/if}
      </div>
    </section>
  {:else if view?.game && status}
    <section class="card table {status.state}" aria-live="polite">
      <div class="row">
        <span class="dot" aria-hidden="true"></span>
        <div class="grow">
          <strong>{HEADLINES[status.state]}</strong>
          <p>
            {status.message}{#if status.finished && (status.state === "ready" || status.state === "none")}<span
                class="muted">{" · "}{time(status.finished)}</span
              >{/if}
          </p>
        </div>
        <button onclick={rebuild} disabled={status.state === "building"}>Rebuild now</button>
      </div>
      {#each status.warnings as w}<p class="note warn">{w}</p>{/each}
      {#if mods?.stamp && mods.stamp.made_by !== view.made_by}
        <p class="note">Last built by {mods.stamp.made_by}.</p>
      {/if}
      {#if status.log.length}
        <button class="link small" onclick={() => (showLog = !showLog)}>
          {showLog ? "Hide details" : "Show details"}
        </button>
        {#if showLog}<pre>{status.log.join("\n")}</pre>{/if}
      {/if}
    </section>

    <section class="card mods">
      <div class="row wrap">
        <h2 class="grow">Mods <span class="muted">{onCount} of {mods?.mods.length ?? 0} on</span></h2>
        <button onclick={addZips} disabled={installing}>Add zip…</button>
        <button onclick={addFolder} disabled={installing}>Add folder…</button>
        <button onclick={() => openFolder("mods")}>Open mods folder</button>
      </div>
      {#if mods && mods.mods.length}
        <ul>
          {#each byName as m (m.folder)}
            <li class="mod" class:off={!m.enabled}>
              <label class="switch">
                <input
                  type="checkbox"
                  checked={m.enabled}
                  disabled={toggling !== null}
                  onchange={(e) => toggle(m, e.currentTarget)}
                  aria-label={`${m.enabled ? "Turn off" : "Turn on"} ${leaf(m.name)}`}
                />
                <span></span>
              </label>
              <div class="grow">
                <div class="name">{leaf(m.name)}</div>
                <div class="muted small">
                  {details(m)}{#if m.folder.includes("/")}{" · in "}{parent(m.folder)}{/if}
                </div>
              </div>
              {#if hiddenFiles[m.folder]}
                <span class="tag warn" title="A mod that sorts later changes the same files: see Conflicts">
                  {plural(hiddenFiles[m.folder], "file")} not used
                </span>
              {/if}
              {#if m.own_size_table}
                <span class="tag" title="It came with its own size table: its larger entries are kept in the one for all mods">
                  own table
                </span>
              {/if}
            </li>
          {/each}
        </ul>
        <p class="muted small hint">Drop a mod's zip or folder anywhere in this window to install it.</p>
        {#if !view.game.program}
          <p class="note">
            Turning a mod off puts a <code>-</code> in front of its folder's name. Longshot and Hookshot skip those
            folders; an emulator may not, so check its own list of mods too.
          </p>
        {/if}
      {:else if mods}
        <div class="empty">
          <p><strong>No mods yet.</strong> Drop a mod's zip or folder here, or use Add zip… / Add folder….</p>
          <p>
            You can also put a mod in the mods folder yourself, as its own folder with its <code>romfs</code> folder
            inside: <code>mods/Some Mod/romfs/…</code>
          </p>
        </div>
      {/if}
    </section>

    {#if mods?.conflicts.length}
      <section class="card conflicts">
        <h2>Conflicts <span class="muted">{mods.conflicts.length}</span></h2>
        <p class="muted small">
          When two mods change the same file, the game uses the one from the mod that sorts last. To change which
          one, rename the mods' folders so they sort the other way (<code>01-…</code>, <code>02-…</code>), or turn
          one off.
        </p>
        <ul>
          {#each mods.conflicts as c (c.used + "|" + c.hidden)}
            {@const key = c.used + "|" + c.hidden}
            <li class="conflict">
              <div class="row">
                <div class="grow">
                  <strong>{leaf(c.used)}</strong> is used over <strong>{leaf(c.hidden)}</strong> for
                  {plural(c.files.length, "file")}
                </div>
                <button class="link small" onclick={() => (openConflicts[key] = !openConflicts[key])}>
                  {openConflicts[key] ? "Hide files" : "Show files"}
                </button>
              </div>
              {#if openConflicts[key]}
                <pre>{c.files.slice(0, SHOWN).join("\n")}{c.files.length > SHOWN
                    ? `\n… and ${c.files.length - SHOWN} more`
                    : ""}</pre>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <footer class="muted small">
      The game reads its mods when it starts: restart it after changing them. A mod whose folder name starts with
      <code>-</code> is off.
    </footer>
  {/if}
</main>

{#if dragging}
  <div class="drop" aria-hidden="true"><div>Drop to install</div></div>
{/if}
