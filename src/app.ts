// Main controller: lanes of project cards, shortcuts, session state, persistence.
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import {
  api,
  type AgentInfo,
  type CardMode,
  type CoffeeLevel,
  type CoffeeStatus,
  type GithubRepo,
  type Layout,
  type Project,
  type PtyExit,
  type SessionState,
  type Update,
  type Usage,
} from "./api";
import { detectLanguage, isLanguage, setLanguage, t, type Language } from "./i18n";
import { actionFor, hints, keysIn, setCustomKeys, type Action } from "./keys";
import { PLATFORM, shortcut } from "./platform";
import { Side } from "./side";
import * as L from "./lanes";
import { label, mark, sizable, splitLine, watchDrag, type Box } from "./drag";
import { Palette } from "./palette";
import { pickColor, PROJECT_COLORS, projectColor } from "./projects";
import { keyOf, nextInstance, parseKey, sessionsLabel } from "./sessions";
import { dropped } from "./address";
import { DEFAULT_HOTKEY, hotkeyLabel } from "./hotkey";
import { repoName } from "./github";
import { Settings, type SettingsContext } from "./settings";
import { applyTheme, terminalTheme, themeById } from "./themes";
import {
  DEFAULT_FONT_SIZE,
  displayState,
  escapeHtml,
  setTerminalFontSize,
  setTerminalTheme,
  Tile,
  type TileAction,
} from "./tile";
import { confirmBox, newProjectBox, pickBox, toast } from "./ui";
import { usageHtml } from "./usage";

const POLL_MS = 3000;
/** desk asks GitHub for a newer release once a day (and from Options on demand). */
const UPDATE_CHECK_MS = 24 * 60 * 60 * 1000;
const UPDATE_KEY = "desk.updateCheckedAt";
const USAGE_MS = 30_000;
/** Room of a shell or ⌘U command in its card, next to an agent's 1. */
const AUX_SIZE = 0.4;
/** A card one under another shows its sessions as tabs below this height per session (px). */
const TIGHT_PX = 100;
/** Within this distance of a lane's side, a dragged card makes a new lane (px). */
const EDGE_PX = 40;
const MAX_LANES = 8;

export class App {
  private layout!: Layout;
  private home = "";
  private projectsDir = "";
  private tiles = new Map<string, Tile>(); // by session key (see sessions.ts)
  private hook = new Map<string, SessionState>(); // by tmux slug
  private running = new Set<string>(); // session keys with a tmux session
  private agents: AgentInfo[] = [];
  private projectCache: Promise<Project[]> | null = null;
  private active: string | null = null;
  private opening = new Map<string, Promise<void>>();
  private earlyExits = new Map<number, string>(); // pty-exit that arrived before attach returned
  private missing: { key: string; lane: number }[] = [];
  private creatingProject = false;
  /** GitHub repos for the new project dialog, fetched at most every few minutes. */
  private githubRepos: { at: number; list: Promise<GithubRepo[]> } | null = null;
  /** One card per project in the grid; its sessions' tiles sit inside. */
  private cards = new Map<string, HTMLElement>();
  private laneEls: HTMLElement[] = [];
  /** Cards showing one session at a time now: set to tabs, or too low for their sessions. */
  private tabbed = new Map<string, string[]>();
  /** Folder of ⌘R conversations (they run there, but sit in a project's card). */
  private chatPath = "";
  /** The card a session belongs to: its project, or for a conversation the card it was opened in. */
  private projectOf = (key: string): string => this.layout.cards[key] ?? parseKey(key).path;
  private usage: Usage[] = [];
  /** A newer desk release, shown as a chip in the top bar. */
  private update: Update | null = null;
  private usageAt = 0;
  private coffee: CoffeeStatus = { level: "off", helperInstalled: false, sleepDisabled: false, stoppedBecause: null };
  private fullscreen: string | null = null;
  private locked = false;
  /** Docked at the top of the screen (⌘`, dropdown.rs). */
  private docked = false;
  private lockEl: HTMLElement | null = null;

  private grid: HTMLElement;
  private side: Side;
  /** With the side panel open: the other sessions as mini cards, left of the active one. */
  private rail: HTMLElement;
  private top: HTMLElement;
  /** Above a fullscreen tile: the open sessions and their ⌘1–9. */
  private strip: HTMLElement;
  private footer: HTMLElement;
  private palette: Palette;
  private settings: Settings;

  constructor(root: HTMLElement) {
    document.documentElement.dataset.platform = PLATFORM; // CSS: room for the macOS traffic lights
    root.innerHTML = `
      <div class="top" data-tauri-drag-region></div>
      <div class="middle"><aside class="rail" hidden></aside><div class="stage"><nav class="strip" hidden></nav><main class="grid"></main></div></div>
      <footer class="bottom"></footer>`;
    this.top = root.querySelector(".top")!;
    this.grid = root.querySelector(".grid")!;
    // Opening, closing or resizing the panel changes the grid's width: lay out again.
    this.rail = root.querySelector(".rail")!;
    this.strip = root.querySelector(".strip")!;
    for (const el of [this.rail, this.strip]) {
      el.addEventListener("click", (e) => {
        const key = (e.target as HTMLElement).closest<HTMLElement>("[data-key]")?.dataset.key;
        if (key) this.setActive(key, true);
      });
    }
    this.side = new Side(() => this.layout && this.render(), {
      project: () => (this.active ? this.projectOf(this.active) : null),
      insert: (text) => {
        const tile = this.tiles.get(this.active ?? "");
        if (tile) tile.type(text);
        else toast(t("app.pickSessionFirst"));
      },
      terminal: (path) => void this.openTerminal(path),
      browser: () => this.layout?.browser ?? "auto",
    });
    root.querySelector(".middle")!.appendChild(this.side.el);
    this.footer = root.querySelector(".bottom")!;

    this.palette = new Palette({
      home: () => this.home,
      projects: () => this.projects(),
      agents: () => this.agents,
      isOpen: (path, agent) => this.tiles.has(keyOf({ path, agent, instance: 0 })),
      isRunning: (path, agent) => this.running.has(keyOf({ path, agent, instance: 0 })),
      isAutostart: (path, agent) => this.layout.autostart.includes(keyOf({ path, agent, instance: 0 })),
      color: (p) => projectColor(p, this.layout.colors),
      open: (path, agent) => void this.openTile(keyOf({ path, agent, instance: 0 })),
      toggleAutostart: (path, agent) => this.toggleAutostart(keyOf({ path, agent, instance: 0 })),
      kill: (path, agent) => void this.killSession(keyOf({ path, agent, instance: 0 })),
      addFolder: () => void this.addFolder(),
      newProject: (agent, name) => void this.newProject(agent, name),
      repos: () => this.repos(),
      projectsDir: () => this.layout.projectsDir ?? this.projectsDir,
      cloneRepo: (repo, agent) => void this.cloneRepo(repo, agent),
      chat: (agent) => void this.openChat(agent),
    });
    this.settings = new Settings({
      home: () => this.home,
      layout: () => this.layout,
      projects: () => this.projects(),
      setLaneCount: (n) => {
        this.layout.lanes = L.setLaneCount(this.layout.lanes, n);
        this.changed();
      },
      setTheme: (id) => {
        this.layout.theme = id;
        this.applyTheme();
        this.save();
      },
      checkUpdate: () => this.checkUpdate(true),
      setFontSize: (px) => this.setFontSize(px),
      setLanguage: (id) => {
        this.layout.language = id;
        setLanguage(id);
        this.save();
        this.relabel();
      },
      setBrowser: (id) => {
        this.layout.browser = id;
        this.save();
        void this.side.browserChanged();
      },
      setDropdown: (change) => this.setDropdown(change),
      setKey: (id, accelerator) => {
        const keys = { ...this.layout.keys };
        if (accelerator) keys[id] = accelerator;
        else delete keys[id];
        this.layout.keys = keys;
        setCustomKeys(keys);
        this.save();
        this.renderChrome();
      },
      setAutoLayout: (on) => {
        this.layout.autoLayout = on;
        this.changed();
      },
      setTmuxMenu: (on) => {
        this.layout.tmuxMenu = on;
        this.save();
      },
      setNotifications: (on) => {
        this.layout.notifications = on;
        this.save();
        if (on) void ensureNotificationPermission();
      },
      toggleAutostart: (p) => this.toggleAutostart(p),
      addFolder: () => this.addFolder(),
      removeFolder: (p) => {
        this.layout.extraFolders = this.layout.extraFolders.filter((f) => f !== p);
        this.projectCache = null;
        this.save();
      },
      coffee: () => this.coffee,
      uninstallCoffee: async () => {
        this.coffee = await api.coffeeUninstall().catch((err) => {
          toast(t("coffee.error", { err: String(err) }));
          return this.coffee;
        });
        this.renderChrome();
      },
      agents: () => this.agents,
      addAgent: async (id, command) => {
        this.layout.agents = [...this.layout.agents.filter((a) => a.id !== id), { id, command }];
        await this.saveNow();
      },
      removeAgent: async (id) => {
        this.layout.agents = this.layout.agents.filter((a) => a.id !== id);
        await this.saveNow();
      },
    });
  }

  async start(): Promise<void> {
    // Listen before reading the snapshot so no state change falls in between.
    await listen<SessionState>("session-state", (e) => this.onHookState(e.payload));
    await listen<PtyExit>("pty-exit", (e) => void this.onPtyExit(e.payload));
    await listen<CoffeeStatus>("coffee", (e) => this.onCoffeeStopped(e.payload));
    // macOS: desk catches ⌘Q / ⌘⇧Q itself, before its menu (Quit) and the Apple menu (Log Out).
    await listen<"close" | "kill">("shortcut", (e) => this.onShortcut(e.payload));
    // Docked at the top of the screen: no traffic lights to make room for.
    await listen<boolean>("dropdown", (e) => {
      this.docked = e.payload;
      document.documentElement.toggleAttribute("data-dropdown", e.payload);
      this.renderChrome();
    });
    // You dragged the dropdown's edge: the next one comes down this tall.
    await listen<number>("dropdown-height", (e) => {
      this.layout.dropdownHeight = e.payload;
      this.save();
      if (this.settings.isOpen) this.settings.render();
    });
    this.footer.addEventListener("click", (e) => {
      if (!(e.target as HTMLElement).closest("[data-pin]")) return;
      void this.setDropdown({ dropdownAutohide: !this.layout.dropdownAutohide }).then(() => {
        if (this.settings.isOpen) this.settings.render();
      });
    });
    this.top.addEventListener("click", (e) => {
      const a = (e.target as HTMLElement).closest<HTMLElement>("[data-coffee]")?.dataset.coffee;
      if (a === "cycle") void this.cycleCoffee();
      if (a === "restore") void this.restoreSleep();
      const t = (e.target as HTMLElement).closest<HTMLElement>("[data-top]")?.dataset.top;
      if (t === "browser") this.side.browse();
      if (t === "update") void this.installUpdate();
      if (t === "leave") void api.dropdownLeave();
    });
    const boot = await api.bootstrap();
    // First launch: the system's language, kept from now on.
    const saved = boot.layout.language ?? "";
    const language: Language = isLanguage(saved) ? saved : detectLanguage();
    setLanguage(language);
    this.side.relabel();
    this.coffee = await api.coffeeStatus();
    this.agents = await api.listAgents();
    this.home = boot.home;
    this.projectsDir = boot.projectsDir;
    this.chatPath = await api.chatDir().catch(() => "");
    for (const s of boot.states) this.hook.set(s.slug, s);
    for (const s of boot.sessions) this.running.add(keyOf(s));
    if (boot.tmuxMissing) {
      const install = PLATFORM === "mac" ? "brew install tmux" : "sudo apt install tmux";
      toast(t("app.noTmux", { install }), 15000);
    }

    this.layout = {
      ...boot.layout,
      colors: boot.layout.colors ?? {},
      agents: boot.layout.agents ?? [],
      runs: boot.layout.runs ?? {},
      // Conversations used to sit in the active project's card: back to their own.
      cards: Object.fromEntries(Object.entries(boot.layout.cards ?? {}).filter(([k]) => parseKey(k).path !== this.chatPath)),
      autoLayout: boot.layout.autoLayout ?? true,
      widths: boot.layout.widths ?? [],
      heights: boot.layout.heights ?? {},
      sizes: boot.layout.sizes ?? {},
      modes: boot.layout.modes ?? {},
      tabs: boot.layout.tabs ?? {},
      theme: boot.layout.theme ?? "desk",
      browser: boot.layout.browser ?? "auto",
      fontSize: boot.layout.fontSize || DEFAULT_FONT_SIZE,
      minimized: boot.layout.minimized ?? [],
      titles: boot.layout.titles ?? {},
      dropdown: boot.layout.dropdown ?? PLATFORM === "mac",
      dropdownHeight: boot.layout.dropdownHeight || 50,
      dropdownAutohide: boot.layout.dropdownAutohide ?? true,
      dropdownHotkey: boot.layout.dropdownHotkey || DEFAULT_HOTKEY(),
      dropdownAnimation: boot.layout.dropdownAnimation ?? 200,
      keys: boot.layout.keys ?? {},
      tmuxMenu: boot.layout.tmuxMenu ?? false,
      language,
      lanes: L.restore(boot.layout.lanes, boot.layout.autostart, boot.layout.lanes.length),
    };
    setCustomKeys(this.layout.keys);
    if (language !== boot.layout.language) this.save();
    this.applyTheme();
    // Another app (iTerm's hotkey window...) may hold ⌘` already.
    if (this.layout.dropdown) void api.dropdownApply().catch((err) => toast(`Dropdown: ${err}`, 10000));
    setTerminalFontSize(this.layout.fontSize);

    window.addEventListener("keydown", (e) => this.onKey(e), true);
    for (const type of ["paste", "drop", "keypress", "input"]) {
      window.addEventListener(type, (e) => this.locked && !isLockField(e.target) && (e.preventDefault(), e.stopPropagation()), true);
    }
    // A file dropped where nothing takes it would replace desk with the file
    // (WebKit opens it), leaving no way back: take every file drop here.
    window.addEventListener("dragover", (e) => {
      if (e.defaultPrevented || !e.dataTransfer?.types.includes("Files")) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = this.tileAt(e.target) ? "copy" : "none";
    });
    window.addEventListener("drop", (e) => {
      if (e.defaultPrevented || !e.dataTransfer?.types.includes("Files")) return;
      e.preventDefault();
      const tile = this.tileAt(e.target);
      if (tile) void this.dropFiles(tile);
    });
    if (await api.isLocked()) this.showLock();
    let resizeTimer: number | undefined;
    window.addEventListener("resize", () => {
      this.fitAll();
      // Cards too low for their sessions turn to tabs: look again once resizing settles.
      clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(() => this.render(), 120);
    });

    // Reattach every tile from the saved layout (starting sessions tmux lost) and the autostart ones.
    const keys = L.flatten(this.layout.lanes);
    const opened = await Promise.all(keys.map((k) => this.createTile(k)));
    keys.forEach((k, i) => {
      if (opened[i]) return;
      // Keep it in the saved layout (volume not mounted, tmux not ready...) for the next start.
      this.missing.push({ key: k, lane: this.layout.lanes.findIndex((l) => l.includes(k)) });
      this.layout.lanes = L.remove(this.layout.lanes, k);
    });
    this.render();
    await Promise.all([...this.tiles.values()].map((t) => t.start().catch((err) => toast(`${t.name}: ${err}`))));
    this.focusIndex(0);
    this.save();
    void this.poll();
    setInterval(() => void this.poll(), POLL_MS);
    void this.side.browserChanged();
    // Hourly look at the clock, so a laptop that slept still checks once a day.
    window.setTimeout(() => void this.checkUpdateDaily(), 10_000);
    setInterval(() => void this.checkUpdateDaily(), 60 * 60 * 1000);
    if (this.layout.notifications) void ensureNotificationPermission();
    if (!this.tiles.size) void this.palette.show();
  }

  // ---- tiles -------------------------------------------------------------

  private async createTile(key: string): Promise<Tile | null> {
    const spec = parseKey(key);
    try {
      const slug = await api.openSession(spec.path, spec.agent, spec.instance, this.layout.runs[key]);
      this.running.add(key);
      this.usageAt = 0; // a new harness may need its limits on the next poll
      if (this.tiles.has(key)) return null; // opened meanwhile (startup restore vs palette)
      const tile = new Tile(spec, slug, (t, a) => void this.onTileAction(t, a));
      tile.detail = this.layout.runs[key] ?? null;
      tile.setName(this.layout.titles[key] ?? null);
      tile.onRename = (name) => this.renameTile(key, name);
      tile.el.querySelector("header")!.addEventListener("pointerdown", (e) => {
        if (!(e.target as HTMLElement).closest("button, input")) this.dragTile(e, key);
      });
      tile.chat = spec.path === this.chatPath;
      // One color per project (shared by its agents and sub-sessions), distinct from other projects.
      const taken = [...this.tiles.values()]
        .filter((t) => t.path !== spec.path)
        .map((t) => this.layout.colors[t.path])
        .filter((i) => i !== undefined);
      this.layout.colors[spec.path] = pickColor(spec.path, this.layout.colors, taken);
      tile.setColor(projectColor(spec.path, this.layout.colors));
      this.tiles.set(key, tile);
      return tile;
    } catch (err) {
      toast(t("app.openFailed", { agent: spec.agent, path: spec.path, err: String(err) }));
      return null;
    }
  }

  openTile(key: string): Promise<void> {
    if (this.tiles.has(key)) {
      this.setActive(key, true);
      return Promise.resolve();
    }
    let pending = this.opening.get(key);
    if (!pending) {
      pending = this.doOpenTile(key).finally(() => this.opening.delete(key));
      this.opening.set(key, pending);
    }
    return pending;
  }

  private async doOpenTile(key: string): Promise<void> {
    this.missing = this.missing.filter((m) => m.key !== key);
    const tile = await this.createTile(key);
    if (!tile) return;
    this.place(key);
    this.render();
    try {
      await tile.start();
    } catch (err) {
      toast(`${tile.name}: ${err}`);
    }
    this.setActive(key, true);
    this.save();
    void this.poll();
  }

  /** A plain conversation, not about any project: a new session each time. */
  private async openChat(agent = "claude"): Promise<void> {
    const path = await api.chatDir();
    this.chatPath = path;
    const taken = [...(await api.listSessions()), ...this.tiles.values()];
    // Its own "rozmowa" card, like a new project (not inside the active one's).
    await this.openTile(keyOf({ path, agent, instance: nextInstance(taken, path, agent) }));
  }

  /** Pick a branch for the project and `git switch` to it (git refuses rather than drop changes). */
  private async switchBranch(path: string): Promise<void> {
    const tiles = [...this.tiles.values()].filter((t) => t.path === path);
    const name = tiles[0]?.name ?? path;
    let branches: string[];
    try {
      branches = await api.gitBranches(path);
    } catch (err) {
      return toast(`${name}: ${err}`);
    }
    const current = tiles[0]?.branch ?? null;
    const choice = (await pickBox(`Branch · ${name}`, branches, { current }))?.value;
    if (!choice || choice === current) return;
    if (tiles.some((t) => t.state === "working")) {
      const ok = await confirmBox(t("app.busyBranch", { name, branch: choice }), t("app.switch"));
      if (!ok) return;
    }
    try {
      await api.gitSwitch(path, choice);
      toast(`${name}: ${choice}`);
    } catch (err) {
      toast(`${name}: ${err}`);
    }
    void this.poll();
  }

  /** ⌘U: run one of the project's commands (dev server, tests...) in its card. */
  private async runInProject(path: string): Promise<void> {
    const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
    const targets = await api.runTargets(path);
    if (!targets.length) return toast(t("app.nothingToRun", { name }));
    const source = new Map(targets.map((t) => [t.command, t.source]));
    const picked = await pickBox(t("common.runPick", { name }), [...source.keys()], { note: (c) => source.get(c) ?? "" });
    if (!picked) return;
    const taken = [...(await api.listSessions()), ...this.tiles.values()];
    const key = keyOf({ path, agent: "run", instance: nextInstance(taken, path, "run") });
    this.layout.runs[key] = picked.value;
    await this.openTile(key);
  }

  /** A plain shell in a project's card (in the home folder with no tile focused). */
  private async openTerminal(path: string): Promise<void> {
    const taken = [...(await api.listSessions()), ...this.tiles.values()];
    await this.openTile(keyOf({ path, agent: "shell", instance: nextInstance(taken, path, "shell") }));
  }

  /** Another session of the same agent in the same project, next to it. */
  private async openSub(tile: Tile): Promise<void> {
    const sessions = await api.listSessions();
    const taken = [...sessions, ...[...this.tiles.values()]];
    const key = keyOf({ path: tile.path, agent: tile.agent, instance: nextInstance(taken, tile.path, tile.agent) });
    await this.openTile(key);
  }

  /** Close the tile; the tmux session keeps running in the background. */
  closeTile(key: string): void {
    const tile = this.tiles.get(key);
    if (!tile) return;
    const order = L.flatten(this.layout.lanes);
    const at = order.indexOf(key);
    tile.dispose();
    this.tiles.delete(key);
    this.layout.lanes = L.remove(this.layout.lanes, key);
    const full = this.fullscreen === key;
    if (full) this.fullscreen = null;
    this.changed();
    const next = order.filter((k) => k !== key)[Math.max(0, at - 1)];
    // Fullscreen stays on, like closing a tab: the neighbour takes the screen.
    if (next && full) this.toggleFullscreen(next);
    else if (next) this.setActive(next, true);
    else this.grid.classList.remove("has-full");
  }

  async killSession(key: string): Promise<void> {
    const spec = parseKey(key);
    const name = spec.path.split(/[\\/]/).pop() ?? spec.path;
    if (!(await confirmBox(t("app.killConfirm", { agent: spec.agent, name }), t("app.kill")))) return;
    const tile = this.tiles.get(key);
    if (tile) this.closeTile(key);
    const slug = tile?.slug ?? (await api.listSessions()).find((s) => keyOf(s) === key)?.slug;
    if (slug) await api.killSession(slug).catch((err) => toast(String(err)));
    this.running.delete(key);
    const forgotten = [this.layout.runs, this.layout.cards, this.layout.titles, this.layout.sizes].some((m) => key in m);
    if (forgotten || this.layout.minimized.includes(key)) {
      delete this.layout.runs[key];
      delete this.layout.cards[key];
      delete this.layout.titles[key];
      delete this.layout.sizes[key];
      this.layout.minimized = this.layout.minimized.filter((k) => k !== key);
      this.save();
    }
    this.palette.refresh();
  }

  private async onTileAction(tile: Tile, action: TileAction): Promise<void> {
    if (action === "focus") {
      // A click on a folded tile's header unfolds it.
      if (tile.isMinimized) this.toggleMinimized(tile.key);
      return this.setActive(tile.key, false);
    }
    if (action === "minimize") return this.toggleMinimized(tile.key);
    if (action === "close") return this.closeTile(tile.key);
    if (action === "kill") return this.killSession(tile.key);
    if (action === "fullscreen") return this.toggleFullscreen(tile.key);
    if (action === "sub") return this.openSub(tile);
    if (action === "terminal") return this.openTerminal(tile.path);
    if (action === "run") return this.runInProject(tile.path);
    if (action === "branch") return this.switchBranch(tile.path);
    if (action === "attached" && tile.ptyId !== null) {
      const slug = this.earlyExits.get(tile.ptyId);
      if (slug !== undefined) {
        this.earlyExits.delete(tile.ptyId);
        return this.onPtyExit({ id: tile.ptyId, slug });
      }
      return;
    }
    if (action === "color") {
      const next = ((this.layout.colors[tile.path] ?? 0) + 1) % PROJECT_COLORS.length;
      this.layout.colors[tile.path] = next;
      for (const t of this.tiles.values()) if (t.path === tile.path) t.setColor(PROJECT_COLORS[next]);
      this.renderCards();
      this.save();
    }
  }

  private async onPtyExit(e: PtyExit): Promise<void> {
    const tile = [...this.tiles.values()].find((t) => t.ptyId === e.id);
    if (!tile) {
      this.earlyExits.set(e.id, e.slug); // attach has not returned its id yet
      return;
    }
    const alive = (await api.listSessions()).some((s) => s.slug === e.slug);
    // The tile may have been closed or replaced while we waited.
    if (this.tiles.get(tile.key) !== tile || tile.ptyId !== e.id) return;
    if (alive) {
      await tile.attach(); // detached from outside (e.g. tmux detach-client): attach again
    } else {
      this.running.delete(tile.key);
      this.closeTile(tile.key);
      toast(t("app.sessionEnded", { name: tile.name }));
    }
  }

  private setActive(key: string, focus: boolean): void {
    // A card showing one session at a time turns to this one.
    const project = this.projectOf(key);
    if (this.tabbed.has(project) && this.tabOf(project, this.tabbed.get(project)!) !== key) {
      this.layout.tabs[project] = key;
      this.save();
      this.render();
    }
    if (this.active !== key) {
      this.tiles.get(this.active ?? "")?.setActive(false);
      this.active = key;
      this.tiles.get(key)?.setActive(true);
      if (this.fullscreen && this.fullscreen !== key) this.toggleFullscreen(key);
      if (this.focusMode()) this.render(); // the newly active session moves to the middle
      this.side.projectChanged();
    }
    if (focus) this.tiles.get(key)?.focus();
  }

  private focusIndex(i: number): void {
    const key = L.flatten(this.layout.lanes)[i];
    if (!key) return;
    if (this.tiles.get(key)?.isMinimized) this.toggleMinimized(key);
    this.setActive(key, true);
  }

  /** A name given by hand ("" clears it): kept in the layout, shown in the header and the rail. */
  private renameTile(key: string, name: string): void {
    const tile = this.tiles.get(key);
    if (name) this.layout.titles[key] = name;
    else delete this.layout.titles[key];
    tile?.setName(name);
    this.save();
    this.render();
  }

  /** Fold a tile to its header (a terminal that only prints logs), or unfold it. */
  private toggleMinimized(key?: string): void {
    const target = key ?? this.active;
    if (!target || !this.tiles.has(target)) return;
    const on = !this.layout.minimized.includes(target);
    this.layout.minimized = on ? [...this.layout.minimized, target] : this.layout.minimized.filter((k) => k !== target);
    if (on && this.fullscreen === target) this.toggleFullscreen(target);
    this.save();
    this.render();
  }

  private toggleFullscreen(key?: string): void {
    const target = key ?? this.active;
    if (target && this.fullscreen !== target && this.layout.minimized.includes(target)) {
      this.layout.minimized = this.layout.minimized.filter((k) => k !== target);
      this.save();
      this.render();
    }
    this.fullscreen = !target || this.fullscreen === target ? null : target;
    this.grid.classList.toggle("has-full", this.fullscreen !== null);
    for (const t of this.tiles.values()) t.el.classList.toggle("full", t.key === this.fullscreen);
    if (target) this.setActive(target, true);
    this.renderStrip();
    this.fitAll();
  }

  /** One tab per session, in ⌘1–9 order; the fullscreen one stands out. */
  private renderStrip(): void {
    const tiles = L.flatten(this.layout.lanes)
      .map((k) => this.tiles.get(k))
      .filter((t): t is Tile => t !== undefined);
    this.strip.hidden = this.fullscreen === null || tiles.length < 2;
    if (this.strip.hidden) return this.strip.replaceChildren();
    this.strip.innerHTML = tiles
      .map((t) => {
        const s = t.summary();
        return `<button class="strip-tab state-${t.state}${t.key === this.fullscreen ? " sel" : ""}" data-key="${escapeHtml(t.key)}" title="${escapeHtml(t.path)}" style="--proj: ${projectColor(this.projectOf(t.key), this.layout.colors)}"><span class="dot"></span><b>${escapeHtml(s.name)}</b> <span class="dim">${escapeHtml(s.label)}</span>${
          t.index > 0 && t.index < 10 ? `<span class="strip-idx">${shortcut(`⌘${t.index}`)}</span>` : ""
        }</button>`;
      })
      .join("");
  }

  // ---- layout ------------------------------------------------------------

  /** Side panel open with several sessions: one in the middle, the rest in the rail. */
  private focusMode(): boolean {
    return this.side.isOpen && this.tiles.size > 1;
  }

  private render(): void {
    const focus = this.focusMode();
    this.layout.lanes = L.group(this.layout.lanes, this.projectOf);
    if (this.layout.autoLayout && !focus) ({ lanes: this.layout.lanes, widths: this.layout.widths } = L.prune(this.layout.lanes, this.layout.widths));
    this.layout.widths = this.layout.lanes.map((_, i) => (this.layout.widths[i] > 0 ? this.layout.widths[i] : 1));
    L.flatten(this.layout.lanes).forEach((p, i) => this.tiles.get(p)?.update({ index: i + 1 }));
    const shown = focus ? this.active ?? L.flatten(this.layout.lanes).find((k) => this.tiles.has(k)) : undefined;
    const lanes = shown ? [[shown]] : this.layout.lanes;
    if (shown) for (const t of this.tiles.values()) if (t.key !== shown) t.el.remove();
    this.tabbed.clear();
    const stacked: { card: HTMLElement; project: string; keys: string[]; open: number }[] = [];
    const grid: HTMLElement[] = [];
    lanes.forEach((paths, i) => {
      if (i > 0) grid.push(splitLine("v", () => this.changed()));
      const lane = (this.laneEls[i] ??= this.laneEl());
      lane.dataset.lane = String(i);
      if (shown) lane.style.flexGrow = "1";
      else sizable(lane, { get: () => this.layout.widths[i], set: (w) => (this.layout.widths[i] = w) });
      const body = lane.querySelector<HTMLElement>(".lane-body")!;
      const items: HTMLElement[] = [lane.querySelector<HTMLElement>(".hint")!];
      const open = paths.filter((p) => this.tiles.has(p));
      // Order inside the lane: one card per project, its sessions in their order.
      L.groups(open, this.projectOf).forEach((keys, j) => {
        if (j > 0) items.push(splitLine("h", () => this.changed()));
        const project = this.projectOf(keys[0]);
        const card = this.card(project);
        const mode: CardMode = shown ? "stack" : (this.layout.modes[project] ?? "stack");
        const unfolded = this.fillCard(card, project, keys, mode, shown);
        if (mode === "stack" && unfolded > 1) stacked.push({ card, project, keys, open: unfolded });
        items.push(card);
      });
      body.replaceChildren(...items);
      body.classList.toggle("empty", open.length === 0);
      grid.push(lane);
    });
    this.grid.replaceChildren(...grid);
    for (const [project, card] of this.cards) {
      if (!card.querySelector(".tile")) {
        card.remove();
        this.cards.delete(project);
      }
    }
    // Laid out: a card too low for its sessions one under another shows them as tabs.
    for (const c of stacked) {
      const height = c.card.querySelector(".card-body")!.getBoundingClientRect().height;
      if (height / c.open < TIGHT_PX) this.showTabs(c.card, c.project, c.keys, true);
    }
    this.renderRail(shown);
    this.renderCards();
    this.renderChrome();
    this.fitAll();
  }

  /** A card's tiles as its mode shows them; returns how many are unfolded. */
  private fillCard(card: HTMLElement, project: string, keys: string[], mode: CardMode, shown?: string): number {
    const multi = keys.length > 1;
    const body = card.querySelector<HTMLElement>(".card-body")!;
    const items: HTMLElement[] = [];
    let unfolded = 0;
    keys.forEach((k, i) => {
      const t = this.tiles.get(k)!;
      t.update({ grouped: multi });
      t.el.classList.remove("tab-hidden");
      // The one session in the middle (side panel open) always shows.
      const folded = k !== shown && mode !== "tabs" && this.layout.minimized.includes(k);
      t.setMinimized(folded);
      if (i > 0 && mode !== "tabs") items.push(splitLine(mode === "row" ? "v" : "h", () => this.changed()));
      sizable(t.el, { get: () => this.sessionSize(k), set: (w) => (this.layout.sizes[k] = w) });
      // Folded tiles take only their header.
      if (folded) t.el.style.flexGrow = "";
      else unfolded++;
      items.push(t.el);
    });
    body.replaceChildren(...items);
    card.classList.toggle("multi", multi);
    card.classList.remove("dragging");
    body.classList.toggle("row", multi && mode === "row");
    card.querySelectorAll<HTMLElement>("[data-mode]").forEach((b) => b.classList.toggle("sel", b.dataset.mode === mode));
    card.querySelector<HTMLElement>(".card-tabs")!.hidden = true;
    if (multi && mode === "tabs") {
      this.showTabs(card, project, keys, false);
      unfolded = 1;
    }
    // A card of folded tiles takes only its headers.
    if (unfolded) {
      card.style.flex = "";
      sizable(card, {
        get: () => this.layout.heights[project] ?? this.cardHeight(keys),
        set: (w) => (this.layout.heights[project] = w),
      });
    } else card.style.flex = "none";
    return unfolded;
  }

  /** One session of the card at a time, the others behind tabs above it. */
  private showTabs(card: HTMLElement, project: string, keys: string[], tight: boolean): void {
    const shown = this.tabOf(project, keys);
    for (const k of keys) {
      const t = this.tiles.get(k)!;
      t.el.classList.toggle("tab-hidden", k !== shown);
      if (k === shown) {
        t.setMinimized(false);
        t.el.style.flexGrow = "1";
      }
    }
    const body = card.querySelector(".card-body")!;
    body.classList.remove("row");
    body.querySelectorAll(":scope > .split").forEach((line) => line.remove());
    const bar = card.querySelector<HTMLElement>(".card-tabs")!;
    bar.hidden = false;
    bar.classList.toggle("tight", tight);
    this.tabbed.set(project, keys);
    this.renderTabs(project);
  }

  /** The tabs of a card showing one session at a time: state, name and ⌘1–9. */
  private renderTabs(project: string): void {
    const keys = this.tabbed.get(project);
    const bar = this.cards.get(project)?.querySelector<HTMLElement>(".card-tabs");
    if (!keys || !bar) return;
    const shown = this.tabOf(project, keys);
    const tabs = keys
      .map((k) => this.tiles.get(k))
      .filter((t): t is Tile => t !== undefined)
      .map((t) => {
        const s = t.summary();
        const idx = t.index > 0 && t.index < 10 ? `<span class="strip-idx">${shortcut(`⌘${t.index}`)}</span>` : "";
        return `<button class="card-tab state-${t.state}${t.key === shown ? " sel" : ""}" data-key="${escapeHtml(t.key)}" title="${escapeHtml(t.path)}"><span class="dot"></span><span class="card-tab-name">${escapeHtml(s.label)}</span>${idx}</button>`;
      });
    const note = bar.classList.contains("tight") ? `<span class="card-tabs-note">${escapeHtml(t("card.tight"))}</span>` : "";
    bar.innerHTML = tabs.join("") + note;
  }

  /** The session a card in tabs shows: the one picked last, else its first. */
  private tabOf(project: string, keys: string[]): string {
    const picked = this.layout.tabs[project];
    return picked && keys.includes(picked) ? picked : keys[0];
  }

  /** Room of a session in its card: as resized, else 1 for an agent and less for a shell or command. */
  private sessionSize(key: string): number {
    const size = this.layout.sizes[key];
    if (size > 0) return size;
    return ["shell", "run"].includes(parseKey(key).agent) ? AUX_SIZE : 1;
  }

  /** A card's height until it is resized: what its unfolded sessions need. */
  private cardHeight(keys: string[]): number {
    const open = keys.filter((k) => !this.layout.minimized.includes(k));
    return open.reduce((sum, k) => sum + this.sessionSize(k), 0) || 1;
  }

  /** Put a newly opened session in the layout; nothing already open moves. */
  private place(key: string): void {
    const project = this.projectOf(key);
    const mates = L.flatten(this.layout.lanes).filter((k) => k !== key && this.projectOf(k) === project);
    // It takes room from its own card only: the card keeps the height it has now.
    if (mates.length && this.layout.heights[project] === undefined) this.layout.heights[project] = this.cardHeight(mates);
    if (mates.length && this.layout.modes[project] === "tabs") this.layout.tabs[project] = key;
    const before = this.layout.lanes.length;
    this.layout.lanes = this.layout.autoLayout
      ? L.place(this.layout.lanes, key, this.projectOf, this.maxLanes())
      : L.add(this.layout.lanes, key, this.projectOf);
    if (this.layout.lanes.length > before) this.layout.widths.push(average(this.layout.widths));
  }

  /** How many lanes of a readable width fit the window. */
  private maxLanes(): number {
    const width = this.grid.clientWidth || window.innerWidth;
    return Math.max(1, Math.min(L.AUTO.maxColumns, Math.floor(width / L.AUTO.minWidth)));
  }

  /** ⌘L: lay every card out once by the window and the sessions; then it is yours again. */
  private arrange(): void {
    if (this.focusMode() || !this.tiles.size) return;
    const grid = this.grid.getBoundingClientRect();
    this.layout.lanes = L.autoLanes(this.layout.lanes, this.projectOf, grid.width || window.innerWidth, grid.height || window.innerHeight);
    this.layout.widths = this.layout.lanes.map(() => 1);
    this.layout.heights = {};
    this.changed();
    toast(t("app.arranged", { n: this.layout.lanes.length }));
  }

  private setMode(project: string, mode: CardMode): void {
    if (mode === "stack") delete this.layout.modes[project];
    else this.layout.modes[project] = mode;
    this.changed();
  }

  // ---- dragging cards and tiles --------------------------------------------

  private canDrag(): boolean {
    return !this.focusMode() && this.fullscreen === null && !this.locked;
  }

  /** Drag a project's card: between cards, or to a lane's side for a lane of its own. */
  private dragCard(e: PointerEvent, project: string): void {
    if (!this.canDrag()) return;
    const card = this.cards.get(project);
    const name = this.cardName(project);
    let target: CardDrop | null = null;
    watchDrag(e, {
      start: () => card?.classList.add("dragging"),
      move: (ev) => {
        target = this.cardDrop(ev, project);
        mark(target?.box ?? null, target?.zone);
        label(target ? `${name} → ${target.label}` : name, ev);
      },
      drop: (ev) => {
        card?.classList.remove("dragging");
        if (ev && target) this.moveCard(project, target);
      },
    });
  }

  private cardDrop(ev: PointerEvent, project: string): CardDrop | null {
    for (const lane of this.grid.querySelectorAll<HTMLElement>(":scope > .lane")) {
      const r = lane.getBoundingClientRect();
      if (ev.clientX < r.left - 3 || ev.clientX > r.right + 3 || ev.clientY < r.top || ev.clientY > r.bottom) continue;
      const at = Number(lane.dataset.lane);
      const cards = [...lane.querySelectorAll<HTMLElement>(":scope > .lane-body > .card")];
      const others = cards.filter((c) => c !== this.cards.get(project));
      // Its own lane already: a new lane would change nothing.
      const alone = cards.length === 1 && !others.length;
      const room = this.layout.lanes.length < MAX_LANES && !alone;
      const newLane = t("drag.newLane");
      if (room && ev.clientX < r.left + EDGE_PX) return { lane: at, index: -1, box: { x: r.left - 1, y: r.top, w: 3, h: r.height }, label: newLane };
      if (room && ev.clientX > r.right - EDGE_PX) return { lane: at + 1, index: -1, box: { x: r.right - 2, y: r.top, w: 3, h: r.height }, label: newLane };
      const where = t("drag.lane", { n: at + 1 });
      if (!others.length) return { lane: at, index: 0, box: { x: r.left + 4, y: r.top + 4, w: r.width - 8, h: r.height - 8 }, zone: true, label: where };
      let index = others.findIndex((c) => {
        const cr = c.getBoundingClientRect();
        return ev.clientY < cr.top + cr.height / 2;
      });
      if (index < 0) index = others.length;
      const y = index < others.length ? others[index].getBoundingClientRect().top - 1 : others[others.length - 1].getBoundingClientRect().bottom - 2;
      return { lane: at, index, box: { x: r.left + 4, y, w: r.width - 8, h: 3 }, label: where };
    }
    return null;
  }

  private moveCard(project: string, to: CardDrop): void {
    const from = this.layout.lanes.findIndex((l) => l.some((k) => this.projectOf(k) === project));
    if (to.index < 0) {
      this.layout.lanes = L.moveGroupToNewLane(this.layout.lanes, project, this.projectOf, to.lane);
      this.layout.widths.splice(to.lane, 0, average(this.layout.widths));
      delete this.layout.heights[project];
    } else {
      // In another lane it takes a height like the cards already there.
      if (to.lane !== from) {
        const cards = L.groups(this.layout.lanes[to.lane], this.projectOf);
        const heights = cards.map((keys) => this.layout.heights[this.projectOf(keys[0])] ?? this.cardHeight(keys));
        if (heights.length) this.layout.heights[project] = average(heights);
        else delete this.layout.heights[project];
      }
      this.layout.lanes = L.moveGroup(this.layout.lanes, project, this.projectOf, to.lane, to.index);
    }
    this.changed();
    const first = L.flatten(this.layout.lanes).find((k) => this.projectOf(k) === project);
    if (first) this.setActive(first, true);
  }

  /** Drag a tile's header: a lone tile moves its card, others change places inside their card. */
  private dragTile(e: PointerEvent, key: string): void {
    const project = this.projectOf(key);
    const mates = L.flatten(this.layout.lanes).filter((k) => this.projectOf(k) === project);
    if (mates.length < 2) return this.dragCard(e, project);
    if (!this.canDrag()) return;
    const card = this.cards.get(project)!;
    const tabs = this.tabbed.has(project);
    const across = tabs || this.layout.modes[project] === "row";
    const el = () => (tabs ? card.querySelector<HTMLElement>(`.card-tab[data-key="${CSS.escape(key)}"]`) : this.tiles.get(key)?.el) ?? null;
    const name = this.tiles.get(key)?.summary().label ?? "";
    let index: number | null = null;
    watchDrag(e, {
      start: () => el()?.classList.add("dragging"),
      move: (ev) => {
        index = null;
        const r = card.getBoundingClientRect();
        if (ev.clientX < r.left || ev.clientX > r.right || ev.clientY < r.top || ev.clientY > r.bottom) {
          mark(null);
          return label(t("drag.staysInCard"), ev);
        }
        const items = [...card.querySelectorAll<HTMLElement>(tabs ? ".card-tab" : ":scope > .card-body > .tile")].filter((x) => x !== el());
        if (!items.length) return;
        let i = items.findIndex((x) => {
          const b = x.getBoundingClientRect();
          return across ? ev.clientX < b.left + b.width / 2 : ev.clientY < b.top + b.height / 2;
        });
        if (i < 0) i = items.length;
        const b = items[Math.min(i, items.length - 1)].getBoundingClientRect();
        const end = i >= items.length;
        mark(across ? { x: (end ? b.right : b.left) - 1, y: b.top, w: 3, h: b.height } : { x: b.left + 2, y: (end ? b.bottom : b.top) - 1, w: b.width - 4, h: 3 });
        label(name, ev);
        index = i;
      },
      drop: (ev) => {
        el()?.classList.remove("dragging");
        if (!ev || index === null) return;
        this.layout.lanes = L.moveInGroup(this.layout.lanes, key, this.projectOf, index);
        this.changed();
      },
    });
  }

  private cardName(project: string): string {
    const own = [...this.tiles.values()].find((t) => t.path === project);
    return own?.name ?? project.split(/[\\/]/).filter(Boolean).pop() ?? project;
  }

  /** Mini cards of the sessions not in the middle (side panel open). */
  private renderRail(shown?: string): void {
    this.rail.hidden = !shown;
    clearInterval(this.railTimer);
    if (!shown) return this.rail.replaceChildren();
    const cards = L.flatten(this.layout.lanes)
      .filter((k) => k !== shown)
      .map((k) => this.tiles.get(k))
      .filter((t): t is Tile => t !== undefined)
      .map((t) => {
        const s = t.summary();
        const lines = t.snapshot().map((l) => `<div class="mini-line">${escapeHtml(l)}</div>`).join("");
        return `<div class="mini state-${t.state}" data-key="${escapeHtml(t.key)}" style="--proj: ${projectColor(this.projectOf(t.key), this.layout.colors)}">
          <div class="mini-head"><span class="dot"></span><b>${escapeHtml(s.name)}</b> <span class="dim">${escapeHtml(s.label)}</span>${
            s.state ? ` <span class="mini-state">${escapeHtml(s.state)}</span>` : ""
          }${t.index > 0 && t.index < 10 ? `<span class="mini-idx">${shortcut(`⌘${t.index}`)}</span>` : ""}</div>${lines}</div>`;
      });
    this.rail.innerHTML = cards.join("");
    // Their terminals keep running out of sight: refresh what the cards show.
    this.railTimer = window.setInterval(() => this.renderRail(this.focusMode() ? shown : undefined), 1500);
  }

  private railTimer: number | undefined;

  private card(project: string): HTMLElement {
    let card = this.cards.get(project);
    if (card) return card;
    card = document.createElement("div");
    card.className = "card";
    card.dataset.project = project;
    card.innerHTML = keysIn(`
      <header class="card-head">
        <span class="dot"></span><span class="card-title"></span>
        <span class="modes">${(["stack", "row", "tabs"] as const).map((m) => `<button data-mode="${m}">${MODE_ICONS[m]}</button>`).join("")}</span>
        <span class="actions"><button data-a="sub">+</button><button data-a="terminal">›_</button><button data-a="run">▶</button></span>
      </header>
      <nav class="card-tabs" hidden></nav>
      <div class="card-body"></div>`);
    const head = card.querySelector<HTMLElement>(".card-head")!;
    cardTitles(head);
    const first = () => {
      const tiles = [...this.tiles.values()].filter((t) => this.projectOf(t.key) === project);
      return tiles.find((t) => t.path === project) ?? tiles[0];
    };
    head.addEventListener("pointerdown", (e) => {
      if (!(e.target as HTMLElement).closest("button")) this.dragCard(e, project);
    });
    head.addEventListener("click", (e) => {
      const t = first();
      if (!t) return;
      const target = e.target as HTMLElement;
      const mode = target.closest<HTMLElement>("[data-mode]")?.dataset.mode as CardMode | undefined;
      if (mode) return this.setMode(project, mode);
      const a = target.closest<HTMLElement>("[data-a]")?.dataset.a;
      if (a === "sub") void this.openSub(t);
      else if (a === "terminal") void this.openTerminal(project);
      else if (a === "branch") void this.switchBranch(project);
      else if (a === "run") void this.runInProject(project);
      else this.setActive(t.key, true);
    });
    const tabs = card.querySelector<HTMLElement>(".card-tabs")!;
    tabs.addEventListener("pointerdown", (e) => {
      const key = (e.target as HTMLElement).closest<HTMLElement>("[data-key]")?.dataset.key;
      if (key) this.dragTile(e, key);
    });
    tabs.addEventListener("click", (e) => {
      const key = (e.target as HTMLElement).closest<HTMLElement>("[data-key]")?.dataset.key;
      if (key) this.setActive(key, true);
    });
    this.cards.set(project, card);
    return card;
  }

  /** Card headers: project color, name, branch and how many sessions it holds. */
  private renderCards(): void {
    for (const [project, card] of this.cards) {
      const tiles = [...this.tiles.values()].filter((t) => this.projectOf(t.key) === project);
      if (!tiles.length) continue;
      card.style.setProperty("--proj", projectColor(project, this.layout.colors));
      const own = tiles.find((t) => t.path === project);
      const name = this.cardName(project);
      const parts = [
        `<b>${escapeHtml(name)}</b>`,
        own?.branch ? `<span class="branch" data-a="branch" title="${t("common.changeBranch")}">${escapeHtml(own.branch)}</span>` : "",
        `<span class="dim">${sessionsLabel(tiles.length)}</span>`,
      ].filter(Boolean);
      const title = card.querySelector<HTMLElement>(".card-title")!;
      title.innerHTML = parts.join(`<span class="sep"> · </span>`);
      title.title = project;
    }
  }

  /** Labels built once (side panel, tiles, cards, lanes) in the language Options just set. */
  private relabel(): void {
    this.side.relabel();
    for (const tile of this.tiles.values()) tile.relabel();
    for (const card of this.cards.values()) cardTitles(card.querySelector<HTMLElement>(".card-head")!);
    for (const lane of this.laneEls) lane.querySelector(".hint")!.textContent = laneHint();
    this.render();
  }

  private tileAt(target: EventTarget | null): Tile | undefined {
    const key = (target as HTMLElement | null)?.closest?.<HTMLElement>(".tile[data-key]")?.dataset.key;
    return key ? this.tiles.get(key) : undefined;
  }

  /** Files dropped on a terminal: their paths typed into it, as Terminal and iTerm do. */
  private async dropFiles(tile: Tile): Promise<void> {
    const paths = await api.droppedPaths().catch(() => []);
    if (!paths.length) return toast(t("app.dropUnsupported"));
    const text = dropped(paths);
    if (!text) return toast(t("side.unsafePath"));
    this.setActive(tile.key, true);
    tile.paste(text);
  }

  private laneEl(): HTMLElement {
    const lane = document.createElement("div");
    lane.className = "lane";
    lane.innerHTML = `<div class="lane-body"><div class="hint">${laneHint()}</div></div>`;
    return lane;
  }

  /** Terminal font size for every tile (8–24 px). */
  private setFontSize(px: number): void {
    const size = Math.min(24, Math.max(8, Math.round(px * 2) / 2));
    this.layout.fontSize = size;
    setTerminalFontSize(size);
    for (const t of this.tiles.values()) t.setFontSize(size);
    this.save();
    this.settings.render();
  }

  /** Window and terminal colors from the chosen theme. */
  private applyTheme(): void {
    const theme = themeById(this.layout.theme);
    applyTheme(theme);
    const term = terminalTheme(theme);
    setTerminalTheme(term);
    for (const t of this.tiles.values()) t.setTheme(term);
  }

  private changed(): void {
    this.render();
    this.save();
  }

  private saveTimer: number | undefined;
  private save(): void {
    clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => void this.persist(), 150);
  }

  /** Write layout.json; tiles that failed to open at startup keep their place. */
  private persist(): Promise<void> {
    const lanes = L.clone(this.layout.lanes);
    for (const m of this.missing) {
      if (!L.contains(lanes, m.key)) lanes[Math.min(Math.max(m.lane, 0), lanes.length - 1)].push(m.key);
    }
    return api.saveLayout({ ...this.layout, lanes }).catch((err) => toast(t("app.saveLayoutError", { err: String(err) })));
  }

  /** Save right away (Rust reads custom agents from the file) and refresh the agent list. */
  private async saveNow(): Promise<void> {
    clearTimeout(this.saveTimer);
    await this.persist();
    this.agents = await api.listAgents();
  }

  private fitAll(): void {
    for (const t of this.tiles.values()) t.queueFit();
  }

  private toggleAutostart(key: string): void {
    const a = this.layout.autostart;
    this.layout.autostart = a.includes(key) ? a.filter((k) => k !== key) : [...a, key];
    this.save();
    this.palette.refresh();
    this.settings.render();
  }

  private async addFolder(): Promise<void> {
    const picked = await openDialog({ directory: true, multiple: false, defaultPath: this.home });
    if (typeof picked !== "string") return;
    if (!this.layout.extraFolders.includes(picked)) this.layout.extraFolders.push(picked);
    this.projectCache = null;
    this.save();
    if (!this.settings.isOpen) await this.openTile(picked);
  }

  private async newProject(agent: string, name?: string): Promise<void> {
    if (this.creatingProject) return; // ⌘N again while the dialog is up
    this.creatingProject = true;
    try {
      await this.createProject(agent, name);
    } finally {
      this.creatingProject = false;
    }
  }

  private async createProject(agent: string, name?: string): Promise<void> {
    const parent = this.layout.projectsDir ?? this.projectsDir;
    const pick = async () => {
      const p = await openDialog({ directory: true, multiple: false, defaultPath: parent });
      return typeof p === "string" ? p : null;
    };
    let local = new Set<string>();
    void this.projects().then((ps) => (local = new Set(ps.map((p) => p.name.toLowerCase()))));
    const isLocal = (folder: string) => local.has(folder.toLowerCase());
    const req = await newProjectBox(parent, pick, agent, { name, repos: this.repos(), isLocal });
    if (!req) return;
    if (req.repo) toast(t("app.cloning", { repo: req.repo }));
    await this.adoptProject(req.parent, agent, () =>
      req.repo ? api.cloneProject(req.parent, req.name, req.repo) : api.createProject(req.parent, req.name, req.git),
    );
  }

  /** ⌘P on a GitHub repo: clone it into the projects folder and open it. */
  private async cloneRepo(repo: GithubRepo, agent: string): Promise<void> {
    const parent = this.layout.projectsDir ?? this.projectsDir;
    toast(t("app.cloning", { repo: repo.fullName }));
    await this.adoptProject(parent, agent, () => api.cloneProject(parent, repoName(repo), repo.fullName));
  }

  /** Make a project folder in `parent`, remember it and open its tile. */
  private async adoptProject(parent: string, agent: string, make: () => Promise<string>): Promise<void> {
    try {
      const path = await make();
      this.layout.projectsDir = parent;
      if (!this.layout.extraFolders.includes(path)) this.layout.extraFolders.push(path);
      this.projectCache = null;
      this.save();
      await this.openTile(keyOf({ path, agent, instance: 0 }));
    } catch (err) {
      toast(t("app.newProjectError", { err: String(err) }));
    }
  }

  private repos(): Promise<GithubRepo[]> {
    if (!this.githubRepos || Date.now() - this.githubRepos.at > 5 * 60_000) {
      const list = api.githubRepos();
      list.catch(() => (this.githubRepos = null)); // try again next time
      this.githubRepos = { at: Date.now(), list };
    }
    return this.githubRepos.list;
  }

  private projects(): Promise<Project[]> {
    if (!this.projectCache) {
      this.projectCache = api.listProjects(this.layout.extraFolders).catch((err) => {
        toast(t("app.discoveryError", { err: String(err) }));
        this.projectCache = null;
        return [];
      });
      // Refresh in the background next time the palette opens.
      const cached = this.projectCache;
      setTimeout(() => {
        if (this.projectCache === cached) this.projectCache = null;
      }, 60_000);
    }
    return this.projectCache;
  }

  // ---- state -------------------------------------------------------------

  private async poll(): Promise<void> {
    const paths = [...new Set([...this.tiles.values()].map((t) => t.path))];
    const [infos, sessions] = await Promise.all([api.tileInfo(paths), api.listSessions()]);
    this.running = new Set(sessions.map((s) => keyOf(s)));
    const branch = new Map(infos.map((i) => [i.path, i.branch]));
    for (const tile of this.tiles.values()) {
      tile.update({ branch: branch.get(tile.path) ?? null, state: displayState(this.hook.get(tile.slug)?.state) });
    }
    if (Date.now() - this.usageAt >= USAGE_MS) await this.refreshUsage();
    this.renderCards();
    this.renderChrome();
  }

  /** Limits only for the harnesses that have a tile open. */
  private async refreshUsage(): Promise<void> {
    this.usageAt = Date.now();
    const agents = [...new Set([...this.tiles.values()].map((t) => t.agent))];
    this.usage = agents.length ? await api.usage(agents).catch(() => this.usage) : [];
  }

  private onHookState(s: SessionState): void {
    const before = this.hook.get(s.slug)?.state;
    this.hook.set(s.slug, s);
    const tile = [...this.tiles.values()].find((t) => t.slug === s.slug);
    if (!tile) return;
    tile.update({ state: displayState(s.state) });
    this.renderChrome();
    if (s.state === "waiting" && before !== "waiting" && this.layout.notifications && !document.hasFocus()) {
      sendNotification({ title: t("app.waitingNotice", { name: tile.name }), body: s.message ?? t("app.waitingBody") });
    }
  }

  private async setDropdown(change: Parameters<SettingsContext["setDropdown"]>[0]): Promise<void> {
    const before = this.layout.dropdownHotkey;
    Object.assign(this.layout, change);
    // Rust reads the dropdown options from the file.
    clearTimeout(this.saveTimer);
    await this.persist();
    await api.dropdownApply().catch(async (err) => {
      toast(`Dropdown: ${err}`, 8000);
      if (change.dropdownHotkey === undefined) return;
      // Taken by another app: keep the one that works.
      this.layout.dropdownHotkey = before;
      await this.persist();
      await api.dropdownApply().catch(() => {});
    });
    this.renderChrome();
  }

  /** The padlock in the bottom bar: the dropdown stays down when another app takes the focus. */
  private pinHtml(): string {
    if (!this.layout.dropdown) return "";
    const pinned = !this.layout.dropdownAutohide;
    // An open shackle while it hides, a closed one while it stays.
    const shackle = pinned ? "M8 11V7a4 4 0 0 1 8 0v4" : "M8 11V7a4 4 0 0 1 7.75-1.4";
    return `<button class="pin${pinned ? " on" : ""}" data-pin title="${t(pinned ? "bottom.pinnedTitle" : "bottom.pinTitle")}" aria-pressed="${pinned}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="${shackle}"/></svg></button>`;
  }

  private renderChrome(): void {
    this.renderStrip();
    for (const project of this.tabbed.keys()) this.renderTabs(project);
    const states = [...this.tiles.values()].map((t) => t.state);
    const count = (s: string) => states.filter((x) => x === s).length;
    const waiting = count("waiting");
    // Docked: the window stays at the top (no drag), and in place of the
    // traffic lights the bar says how to put desk away or leave the dropdown.
    const drag = this.docked ? "" : " data-tauri-drag-region";
    this.top.toggleAttribute("data-tauri-drag-region", !this.docked);
    const hotkey = escapeHtml(hotkeyLabel(this.layout.dropdownHotkey || DEFAULT_HOTKEY()));
    const leave = this.docked
      ? `<button class="top-btn leave" data-top="leave" title="${t("top.leaveTitle", { hotkey })}">${t("top.leave")}</button>`
      : "";
    const hide = this.docked
      ? `<span class="dim"> · </span><span class="dropdown-hint" title="${t("top.hideTitle")}"><kbd>${hotkey}</kbd> ${t("top.hide")}</span>`
      : "";
    this.top.innerHTML = `${leave}<span class="brand"${drag}>desk</span>${hide}<span class="dim"${drag}> · ${this.tiles.size} sessions</span>${
      waiting ? `<span class="waiting-pill"${drag}>${t("top.waiting", { n: waiting })}</span>` : ""
    }<span class="spacer"${drag}></span>${this.updateHtml()}${usageHtml(this.usage, Date.now())}<button class="top-btn" data-top="browser" title="${t("top.browserTitle", { keys: keysIn("⌘W") })}">${t("top.browser")}</button>${this.coffeeHtml()}`;
    this.footer.innerHTML =
      `<span class="hints">${hints()
        .map(([k, label]) => `<span><kbd>${k}</kbd> ${escapeHtml(label)}</span>`)
        .join("")}</span><span class="counts"><b>${this.tiles.size}</b> sessions · <span class="c-working">${count("working")} working</span> · <span class="c-waiting">${waiting} waiting</span> · ${count("idle")} idle</span>${this.pinHtml()}`;
  }

  // ---- updates -----------------------------------------------------------

  private async checkUpdateDaily(): Promise<void> {
    let last = 0;
    try {
      last = Number(localStorage.getItem(UPDATE_KEY)) || 0;
    } catch {
      // not remembered: check now
    }
    if (Date.now() - last < UPDATE_CHECK_MS) return;
    try {
      localStorage.setItem(UPDATE_KEY, String(Date.now()));
    } catch {
      // checked again next hour
    }
    await this.checkUpdate(false);
  }

  /** `manual` (Options): say what came out, errors included; else quiet unless there is news. */
  private async checkUpdate(manual: boolean): Promise<void> {
    let u: Update | null;
    try {
      u = await api.updateCheck();
    } catch (err) {
      if (manual) toast(t("update.checkError", { err: String(err) }));
      return;
    }
    const news = u !== null && u.version !== this.update?.version;
    this.update = u;
    this.renderChrome();
    if (news) toast(t("update.news", { version: u!.version }));
    else if (manual) toast(u ? t("update.waiting", { version: u.version }) : t("update.latest", { version: await getVersion() }));
  }

  private updateHtml(): string {
    const u = this.update;
    if (!u) return "";
    const title = u.installable ? t("update.installTitle") : t("update.openTitle");
    return `<button class="top-btn update" data-top="update" title="${title}">↑ ${escapeHtml(u.version)}</button>`;
  }

  private async installUpdate(): Promise<void> {
    const u = this.update;
    if (!u) return;
    if (!u.installable) {
      return void api.openExternal(u.url, this.layout.browser).catch((err) => toast(t("update.error", { err: String(err) })));
    }
    const sessions =
      PLATFORM === "windows" ? t("update.sessionsEnd") : t("update.sessionsStay");
    if (!(await confirmBox(t("update.confirm", { version: u.version, sessions }), t("update.install"))))
      return;
    toast(t("update.downloading", { version: u.version }));
    try {
      await api.updateInstall(u.version);
    } catch (err) {
      toast(t("update.error", { err: String(err) }));
    }
  }

  // ---- coffee ------------------------------------------------------------

  private async cycleCoffee(): Promise<void> {
    const next: Record<CoffeeLevel, CoffeeLevel> = { off: "awake", awake: "lid", lid: "off" };
    try {
      this.coffee = await api.coffeeSet(next[this.coffee.level]);
    } catch (err) {
      toast(t("coffee.error", { err: String(err) }));
      if (next[this.coffee.level] === "lid") this.coffee = await api.coffeeSet("off").catch(() => this.coffee);
    }
    this.renderChrome();
  }

  private async restoreSleep(): Promise<void> {
    try {
      this.coffee = await api.coffeeRestore();
    } catch (err) {
      toast(t("coffee.sleepError", { err: String(err) }));
    }
    this.renderChrome();
  }

  private onCoffeeStopped(s: CoffeeStatus): void {
    this.coffee = s;
    this.renderChrome();
    if (s.stoppedBecause) {
      const msg = t("coffee.stopped", { reason: s.stoppedBecause });
      toast(msg);
      sendNotification({ title: "desk", body: msg });
    }
  }

  private coffeeHtml(): string {
    const c = this.coffee;
    const label = { off: "☕", awake: t("coffee.awake"), lid: t("coffee.lid") }[c.level];
    const title = {
      off: t("coffee.offTitle"),
      awake: PLATFORM === "mac" && !c.helperInstalled ? t("coffee.awakeTitleAsk") : t("coffee.awakeTitle"),
      lid: t("coffee.lidTitle"),
    }[c.level];
    const stray =
      c.sleepDisabled && c.level !== "lid"
        ? `<button class="coffee warn" data-coffee="restore" title="${t("coffee.strayTitle")}">${t("coffee.stray")}</button>`
        : "";
    return `${stray}<button class="coffee ${c.level}" data-coffee="cycle" title="${keysIn(title)}">${label}</button>`;
  }

  // ---- lock --------------------------------------------------------------

  private async lock(): Promise<void> {
    try {
      await api.lockDesk();
    } catch (err) {
      toast(t("lock.error", { err: String(err) }));
      return;
    }
    this.palette.close();
    this.settings.close();
    if (this.coffee.level === "off") void this.cycleCoffee(); // keep the screen on while locked
    this.showLock();
  }

  private showLock(): void {
    this.locked = true;
    (document.activeElement as HTMLElement | null)?.blur();
    this.lockEl?.remove();
    this.lockEl = document.createElement("div");
    this.lockEl.className = "lock-screen";
    // Linux checks the account password typed here; macOS and Windows show their own dialog.
    const how = { mac: t("lock.touchId"), windows: "Windows Hello", linux: t("lock.password") }[PLATFORM];
    const field = PLATFORM === "linux" ? `<input type="password" class="lock-password" placeholder="${t("lock.password")}" autocomplete="current-password" />` : "";
    this.lockEl.innerHTML = `<div class="lock-box">${t("lock.locked")} ${field}<button data-unlock>${t("lock.unlock")} · ${how} <kbd>↵</kbd></button></div>`;
    this.lockEl.addEventListener("click", (e) => {
      if ((e.target as HTMLElement).closest("[data-unlock]")) void this.unlock();
    });
    document.body.appendChild(this.lockEl);
    this.lockEl.querySelector<HTMLInputElement>(".lock-password")?.focus();
  }

  private async unlock(): Promise<void> {
    const field = this.lockEl?.querySelector<HTMLInputElement>(".lock-password");
    try {
      await api.unlockDesk(field ? field.value : undefined);
    } catch (err) {
      toast(t("lock.failed", { err: String(err) }));
      if (field) {
        field.value = "";
        field.focus();
      }
      return;
    }
    this.locked = false;
    this.lockEl?.remove();
    this.lockEl = null;
    if (this.active) this.setActive(this.active, true);
  }

  // ---- keys --------------------------------------------------------------

  private onKey(e: KeyboardEvent): void {
    if (this.locked) {
      // Nothing reaches the terminals; Enter unlocks. The password field (Linux) takes typing.
      if (isLockField(e.target)) {
        e.stopPropagation();
        if (e.key === "Enter" && e.type === "keydown") void this.unlock();
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Enter" && e.type === "keydown") void this.unlock();
      return;
    }
    if (this.settings.isRecording) return; // the dropdown's new shortcut
    const action = actionFor(e);
    if (!action) return;
    const overlay = document.querySelector(".overlay") !== null;
    if (overlay && !["palette", "new", "settings", "close"].includes(action.type)) return; // e.g. ⌘↵ in the palette
    e.preventDefault();
    e.stopPropagation();
    this.run(action);
  }

  /** ⌘Q / ⌘⇧Q from desk's key monitor, under the same rules as a key press. */
  private onShortcut(type: "close" | "kill"): void {
    if (this.locked || this.settings.isRecording) return;
    // The monitor reports the keys: Options may have bound ⌘Q to another action.
    const action = actionFor({ key: "q", metaKey: true, shiftKey: type === "kill", altKey: false, ctrlKey: false });
    if (!action) return;
    const overlay = document.querySelector(".overlay") !== null;
    if (overlay && !["palette", "new", "settings", "close"].includes(action.type)) return;
    this.run(action);
  }

  private run(a: Action): void {
    switch (a.type) {
      case "palette":
        this.settings.close();
        void this.palette.show();
        break;
      case "new":
        this.settings.close();
        this.palette.close();
        void this.newProject("claude");
        break;
      case "settings":
        this.palette.close();
        void this.settings.toggle();
        break;
      case "preview":
        this.side.toggle();
        break;
      case "font":
        this.setFontSize(a.step === 0 ? DEFAULT_FONT_SIZE : this.layout.fontSize + a.step);
        break;
      case "close":
        if (this.palette.isOpen) this.palette.close();
        else if (this.settings.isOpen) this.settings.close();
        else if (this.active) this.closeTile(this.active);
        break;
      case "kill":
        if (this.active) void this.killSession(this.active);
        break;
      case "coffee":
        void this.cycleCoffee();
        break;
      case "lock":
        void this.lock();
        break;
      case "chat":
        void this.openChat();
        break;
      case "sub": {
        const t = this.tiles.get(this.active ?? "");
        if (t) void this.openSub(t);
        break;
      }
      case "terminal":
        void this.openTerminal(this.tiles.get(this.active ?? "")?.path ?? this.home);
        break;
      case "files":
        this.side.showFiles();
        break;
      case "run": {
        const tile = this.tiles.get(this.active ?? "");
        if (!tile) {
          toast(t("app.pickTileFirst"));
          break;
        }
        void this.runInProject(tile.path);
        break;
      }
      case "focus":
        this.focusIndex(a.index);
        break;
      case "fullscreen":
        this.toggleFullscreen();
        break;
      case "minimize":
        this.toggleMinimized();
        break;
      case "arrange":
        this.arrange();
        break;
      case "exitFullscreen":
        if (this.fullscreen) this.toggleFullscreen(this.fullscreen);
        break;
    }
  }
}

/** The card header buttons' tooltips, in the current language. */
function cardTitles(head: HTMLElement): void {
  const titles: [string, string][] = [
    ["sub", t("card.sub")],
    ["terminal", t("tile.terminal")],
    ["run", t("tile.run")],
  ];
  for (const [a, title] of titles) head.querySelector<HTMLElement>(`[data-a="${a}"]`)!.title = keysIn(title);
  for (const mode of ["stack", "row", "tabs"] as const) head.querySelector<HTMLElement>(`[data-mode="${mode}"]`)!.title = t(`card.${mode}`);
}

function laneHint(): string {
  return t("lane.hint", { keys: keysIn("⌘P") });
}

/** Where a dragged card lands: before the `index`-th card of `lane`, or in a new lane there (index -1). */
interface CardDrop {
  lane: number;
  index: number;
  box: Box;
  zone?: boolean;
  label: string;
}

function average(xs: number[]): number {
  return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : 1;
}

/** The card header's mode buttons: sessions one under another, side by side, one at a time. */
const MODE_ICONS: Record<CardMode, string> = {
  stack: `<svg viewBox="0 0 12 12"><rect x="1.5" y="1.5" width="9" height="9" rx="1"/><path d="M1.5 6h9"/></svg>`,
  row: `<svg viewBox="0 0 12 12"><rect x="1.5" y="1.5" width="9" height="9" rx="1"/><path d="M6 1.5v9"/></svg>`,
  tabs: `<svg viewBox="0 0 12 12"><rect x="1.5" y="3.5" width="9" height="7" rx="1"/><path d="M1.5 3.5V1.5h4v2"/></svg>`,
};

async function ensureNotificationPermission(): Promise<void> {
  try {
    if (!(await isPermissionGranted())) await requestPermission();
  } catch {
    // Notifications are optional.
  }
}

/** The lock screen's password field (Linux): the one input that works while locked. */
function isLockField(target: EventTarget | null): boolean {
  return target instanceof HTMLElement && target.classList.contains("lock-password");
}
