<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";

  type Game = { dir: string; romfs: string; mods: string; version: string | null };
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
  type ModsView = { mods: Mod[]; table: string | null; stamp: { rules: number; made_by: string } | null };

  let view = $state<StateView | null>(null);
  let mods = $state<ModsView | null>(null);
  let error = $state<string | null>(null);
  let toggling = $state<string | null>(null);
  let choosing = $state(false);
  let showLog = $state(false);

  const status = $derived(view?.status);
  const onCount = $derived(mods ? mods.mods.filter((m) => m.enabled).length : 0);

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
    const dir = await open({ directory: true, title: "Choose your Longshot folder" });
    if (typeof dir !== "string") return;
    error = null;
    choosing = true;
    try {
      view = await invoke<StateView>("choose_game_folder", { dir });
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

  function rebuild() {
    invoke("rebuild_now").catch((e) => (error = String(e)));
  }

  function openFolder(which: "mods" | "game") {
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

  onMount(() => {
    load();
    const stop = listen<TableStatus>("table", (e) => {
      if (view) view.status = e.payload;
      loadMods();
    });
    return () => {
      stop.then((f) => f());
    };
  });
</script>

<main>
  <header>
    <div class="grow">
      <h1>Mod Manager for Longshot</h1>
      {#if view?.game}
        <p class="sub">
          {#if view.game.version}Longshot {view.game.version}{" · "}{/if}<button
            class="link"
            onclick={() => openFolder("game")}
            title="Open the game folder">{view.game.dir}</button
          >
        </p>
      {/if}
    </div>
    {#if view?.game}
      <button onclick={chooseFolder} disabled={choosing}>Change folder</button>
    {/if}
  </header>

  {#if error}
    <div class="banner" role="alert">
      <span class="grow">{error}</span>
      <button class="link" onclick={() => (error = null)}>Dismiss</button>
    </div>
  {/if}

  {#if view && !view.game}
    <section class="card welcome">
      <h2>Where's Longshot?</h2>
      <p>
        Choose the folder Longshot is installed in (the one with <code>LongshotLauncher.exe</code>), or the game
        folder inside it. A <code>mods</code> folder is made beside the game if there isn't one yet.
      </p>
      {#if view.problem}<p class="note warn">{view.problem}</p>{/if}
      <button class="primary" onclick={chooseFolder} disabled={choosing}>
        {choosing ? "Looking…" : "Choose your Longshot folder"}
      </button>
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
      <div class="row">
        <h2 class="grow">Mods <span class="muted">{onCount} of {mods?.mods.length ?? 0} on</span></h2>
        <button onclick={() => openFolder("mods")}>Open mods folder</button>
      </div>
      {#if mods && mods.mods.length}
        <ul>
          {#each mods.mods as m (m.folder)}
            <li class:off={!m.enabled}>
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
              {#if m.own_size_table}
                <span class="tag" title="It came with its own size table: its larger entries are kept in the one for all mods">
                  own table
                </span>
              {/if}
            </li>
          {/each}
        </ul>
      {:else if mods}
        <div class="empty">
          <p><strong>No mods yet.</strong> Put each mod in the mods folder as its own folder, with its <code>romfs</code> folder inside:</p>
          <pre>mods/Some Mod/romfs/…</pre>
          <p>It shows up here, and the size table is updated by itself.</p>
        </div>
      {/if}
    </section>

    <footer class="muted small">
      Longshot reads its mods when it starts: restart the game after changing them. A mod whose folder name starts
      with <code>-</code> is off.
    </footer>
  {/if}
</main>
