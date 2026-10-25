// Main controller: lanes of tiles, shortcuts, session state, persistence.
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import {
  api,
  type AgentInfo,
  type CoffeeLevel,
  type CoffeeStatus,
  type Layout,
  type Project,
  type PtyExit,
  type SessionState,
  type Usage,
} from "./api";
import { actionFor, hints, type Action } from "./keys";
import { PLATFORM, shortcut, shortcuts } from "./platform";
import { Side } from "./side";
import * as L from "./lanes";
import { Palette } from "./palette";
import { pickColor, PROJECT_COLORS, projectColor } from "./projects";
import { keyOf, nextInstance, parseKey, sessionsLabel } from "./sessions";
import { Settings } from "./settings";
import { applyTheme, terminalTheme, themeById } from "./themes";
import { displayState, escapeHtml, setTerminalTheme, Tile, type TileAction } from "./tile";
import { confirmBox, newProjectBox, pickBox, toast } from "./ui";
import { usageHtml } from "./usage";

const POLL_MS = 3000;
const USAGE_MS = 30_000;

export class App {
  private layout!: Layout;
  private home = "";
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
  /** One card per project in the grid; its sessions' tiles sit inside. */
  private cards = new Map<string, HTMLElement>();
  /** Folder of ⌘R conversations (they run there, but sit in a project's card). */
  private chatPath = "";
  /** The card a session belongs to: its project, or for a conversation the card it was opened in. */
  private projectOf = (key: string): string => this.layout.cards[key] ?? parseKey(key).path;
  private usage: Usage[] = [];
  private usageAt = 0;
  private coffee: CoffeeStatus = { level: "off", helperInstalled: false, sleepDisabled: false, stoppedBecause: null };
  private fullscreen: string | null = null;
  private locked = false;
  private lockEl: HTMLElement | null = null;

  private grid: HTMLElement;
  private side: Side;
  /** With the side panel open: the other sessions as mini cards, left of the active one. */
  private rail: HTMLElement;
  private top: HTMLElement;
  private footer: HTMLElement;
  private palette: Palette;
  private settings: Settings;

  constructor(root: HTMLElement) {
    document.documentElement.dataset.platform = PLATFORM; // CSS: room for the macOS traffic lights
    root.innerHTML = `
      <div class="top" data-tauri-drag-region></div>
      <div class="middle"><aside class="rail" hidden></aside><main class="grid"></main></div>
      <footer class="bottom"></footer>`;
    this.top = root.querySelector(".top")!;
    this.grid = root.querySelector(".grid")!;
    // Opening, closing or resizing the panel changes the grid's width: lay out again.
    this.rail = root.querySelector(".rail")!;
    this.rail.addEventListener("click", (e) => {
      const key = (e.target as HTMLElement).closest<HTMLElement>("[data-key]")?.dataset.key;
      if (key) this.setActive(key, true);
    });
    this.side = new Side(() => this.layout && this.render(), {
      project: () => (this.active ? this.projectOf(this.active) : null),
      insert: (text) => {
        const t = this.tiles.get(this.active ?? "");
        if (t) t.type(text);
        else toast("Najpierw wybierz sesję, do której dodać ścieżkę");
      },
      terminal: (path) => void this.openTerminal(path),
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
      newProject: (agent) => void this.newProject(agent),
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
      setAutoLayout: (on) => {
        this.layout.autoLayout = on;
        this.changed();
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
          toast(`Kawa: ${err}`);
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
    this.top.addEventListener("click", (e) => {
      const a = (e.target as HTMLElement).closest<HTMLElement>("[data-coffee]")?.dataset.coffee;
      if (a === "cycle") void this.cycleCoffee();
      if (a === "restore") void this.restoreSleep();
    });
    const boot = await api.bootstrap();
    this.coffee = await api.coffeeStatus();
    this.agents = await api.listAgents();
    this.home = boot.home;
    this.chatPath = await api.chatDir().catch(() => "");
    for (const s of boot.states) this.hook.set(s.slug, s);
    for (const s of boot.sessions) this.running.add(keyOf(s));

    this.layout = {
      ...boot.layout,
      colors: boot.layout.colors ?? {},
      agents: boot.layout.agents ?? [],
      runs: boot.layout.runs ?? {},
      cards: boot.layout.cards ?? {},
      autoLayout: boot.layout.autoLayout ?? true,
      theme: boot.layout.theme ?? "desk",
      lanes: L.restore(boot.layout.lanes, boot.layout.autostart, boot.layout.lanes.length),
    };
    this.applyTheme();

    window.addEventListener("keydown", (e) => this.onKey(e), true);
    for (const type of ["paste", "drop", "keypress", "input"]) {
      window.addEventListener(type, (e) => this.locked && !isLockField(e.target) && (e.preventDefault(), e.stopPropagation()), true);
    }
    if (await api.isLocked()) this.showLock();
    let resizeTimer: number | undefined;
    window.addEventListener("resize", () => {
      this.fitAll();
      // Columns follow the window: re-lay out once resizing settles.
      clearTimeout(resizeTimer);
      if (this.layout.autoLayout) resizeTimer = window.setTimeout(() => this.render(), 120);
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
      toast(`Nie udało się otworzyć ${spec.agent} w ${spec.path}: ${err}`);
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
    this.layout.lanes = L.add(this.layout.lanes, key, this.projectOf);
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
    const key = keyOf({ path, agent, instance: nextInstance(taken, path, agent) });
    // Next to what you are working on, like ⌘D; it still runs in its own folder.
    const active = this.tiles.get(this.active ?? "");
    if (active) this.layout.cards[key] = this.projectOf(active.key);
    await this.openTile(key);
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
      const ok = await confirmBox(`W ${name} sesja właśnie pracuje. Zmienić branch na ${choice}?`, "Zmień");
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
    if (!targets.length) return toast(`${name}: nie znalazłem nic do uruchomienia (package.json, Cargo.toml, Makefile...)`);
    const source = new Map(targets.map((t) => [t.command, t.source]));
    const picked = await pickBox(`Uruchom · ${name}`, [...source.keys()], { note: (c) => source.get(c) ?? "" });
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
    if (this.fullscreen === key) this.fullscreen = null;
    this.changed();
    const next = order.filter((k) => k !== key)[Math.max(0, at - 1)];
    if (next) this.setActive(next, true);
  }

  async killSession(key: string): Promise<void> {
    const spec = parseKey(key);
    const name = spec.path.split(/[\\/]/).pop();
    if (!(await confirmBox(`Zabić sesję ${spec.agent} w „${name}”? Bieżąca rozmowa zostanie przerwana.`, "Zabij"))) return;
    const tile = this.tiles.get(key);
    if (tile) this.closeTile(key);
    const slug = tile?.slug ?? (await api.listSessions()).find((s) => keyOf(s) === key)?.slug;
    if (slug) await api.killSession(slug).catch((err) => toast(String(err)));
    this.running.delete(key);
    if (this.layout.runs[key] || this.layout.cards[key]) {
      delete this.layout.runs[key];
      delete this.layout.cards[key];
      this.save();
    }
    this.palette.refresh();
  }

  private async onTileAction(tile: Tile, action: TileAction): Promise<void> {
    if (action === "focus") return this.setActive(tile.key, false);
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
      toast(`Sesja ${tile.name} zakończona`);
    }
  }

  private setActive(key: string, focus: boolean): void {
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
    if (key) this.setActive(key, true);
  }

  private toggleFullscreen(key?: string): void {
    const target = key ?? this.active;
    this.fullscreen = !target || this.fullscreen === target ? null : target;
    this.grid.classList.toggle("has-full", this.fullscreen !== null);
    for (const t of this.tiles.values()) t.el.classList.toggle("full", t.key === this.fullscreen);
    if (target) this.setActive(target, true);
    this.fitAll();
  }

  // ---- layout ------------------------------------------------------------

  /** Side panel open with several sessions: one in the middle, the rest in the rail. */
  private focusMode(): boolean {
    return this.side.isOpen && this.tiles.size > 1;
  }

  private render(): void {
    const focus = this.focusMode();
    this.layout.lanes =
      this.layout.autoLayout && !focus
        ? L.autoLanes(this.layout.lanes, this.projectOf, this.grid.clientWidth || window.innerWidth, this.grid.clientHeight || window.innerHeight)
        : L.group(this.layout.lanes, this.projectOf);
    const shown = focus ? this.active ?? L.flatten(this.layout.lanes).find((k) => this.tiles.has(k)) : undefined;
    const lanes = shown ? [[shown]] : this.layout.lanes;
    if (shown) for (const t of this.tiles.values()) if (t.key !== shown) t.el.remove();
    while (this.grid.children.length > lanes.length) this.grid.lastElementChild!.remove();
    while (this.grid.children.length < lanes.length) this.grid.appendChild(this.laneEl());
    lanes.forEach((paths, i) => {
      const lane = this.grid.children[i] as HTMLElement;
      lane.dataset.lane = String(i);
      lane.querySelector(".lane-head")!.innerHTML =
        `LANE ${i + 1} <span class="dim">${paths.length || ""}</span>`;
      const body = lane.querySelector<HTMLElement>(".lane-body")!;
      const open = paths.filter((p) => this.tiles.has(p));
      // Order inside the lane: one card per project, its sessions top to bottom.
      for (const keys of L.groups(open, this.projectOf)) {
        const card = this.card(this.projectOf(keys[0]));
        const cardBody = card.querySelector<HTMLElement>(".card-body")!;
        for (const k of keys) {
          const t = this.tiles.get(k)!;
          cardBody.appendChild(t.el);
          t.update({ grouped: keys.length > 1 });
        }
        card.classList.toggle("multi", keys.length > 1);
        card.style.flexGrow = String(keys.length);
        body.appendChild(card);
      }
      body.classList.toggle("empty", open.length === 0);
    });
    for (const [project, card] of this.cards) {
      if (!card.querySelector(".tile")) {
        card.remove();
        this.cards.delete(project);
      }
    }
    L.flatten(this.layout.lanes).forEach((p, i) => this.tiles.get(p)?.update({ index: i + 1 }));
    this.renderRail(shown);
    this.renderCards();
    this.renderChrome();
    this.fitAll();
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
    card.innerHTML = shortcuts(`
      <header class="card-head" draggable="true">
        <span class="dot"></span><span class="card-title"></span>
        <span class="actions"><button data-a="sub" title="Kolejna sesja w tym projekcie (⌘D)">+</button><button data-a="terminal" title="Terminal w tym projekcie (⌘T)">›_</button><button data-a="run" title="Uruchom w tym projekcie, np. dev serwer (⌘U)">▶</button></span>
      </header>
      <div class="card-body"></div>`);
    const head = card.querySelector<HTMLElement>(".card-head")!;
    const first = () => {
      const tiles = [...this.tiles.values()].filter((t) => this.projectOf(t.key) === project);
      return tiles.find((t) => t.path === project) ?? tiles[0];
    };
    head.addEventListener("dragstart", (e) => {
      const t = first();
      if (!t) return;
      e.dataTransfer?.setData("text/x-desk-key", t.key);
      e.dataTransfer!.effectAllowed = "move";
      card.classList.add("dragging");
    });
    head.addEventListener("dragend", () => card.classList.remove("dragging"));
    head.addEventListener("click", (e) => {
      const t = first();
      if (!t) return;
      const a = (e.target as HTMLElement).closest<HTMLElement>("[data-a]")?.dataset.a;
      if (a === "sub") void this.openSub(t);
      else if (a === "terminal") void this.openTerminal(project);
      else if (a === "branch") void this.switchBranch(project);
      else if (a === "run") void this.runInProject(project);
      else this.setActive(t.key, true);
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
      const name = own?.name ?? project.split(/[\\/]/).filter(Boolean).pop() ?? project;
      const parts = [
        `<b>${escapeHtml(name)}</b>`,
        own?.branch ? `<span class="branch" data-a="branch" title="Zmień branch">${escapeHtml(own.branch)}</span>` : "",
        `<span class="dim">${sessionsLabel(tiles.length)}</span>`,
      ].filter(Boolean);
      const title = card.querySelector<HTMLElement>(".card-title")!;
      title.innerHTML = parts.join(`<span class="sep"> · </span>`);
      title.title = project;
    }
  }

  private laneEl(): HTMLElement {
    const lane = document.createElement("div");
    lane.className = "lane";
    lane.innerHTML = `<div class="lane-head"></div><div class="lane-body"><div class="hint">${shortcut("⌘P")} — otwórz projekt</div></div>`;
    const body = lane.querySelector<HTMLElement>(".lane-body")!;
    body.addEventListener("dragover", (e) => {
      if (!e.dataTransfer?.types.includes("text/x-desk-key")) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      this.markDrop(body, dropIndex(body, e.clientY));
    });
    body.addEventListener("dragleave", (e) => {
      if (!body.contains(e.relatedTarget as Node)) this.markDrop(null, 0);
    });
    body.addEventListener("drop", (e) => {
      const key = e.dataTransfer?.getData("text/x-desk-key");
      this.markDrop(null, 0);
      if (!key) return;
      e.preventDefault();
      const laneIdx = Number(lane.dataset.lane);
      const index = dropIndex(body, e.clientY);
      // A project moves as a whole card, with all its sessions.
      this.layout.lanes = L.moveGroup(this.layout.lanes, this.projectOf(key), this.projectOf, laneIdx, index);
      this.changed();
      this.setActive(key, true);
    });
    return lane;
  }

  private markDrop(body: HTMLElement | null, index: number): void {
    this.grid.querySelectorAll(".drop-before, .drop-end").forEach((el) => el.classList.remove("drop-before", "drop-end"));
    if (!body) return;
    const cards = droppable(body);
    if (cards[index]) cards[index].classList.add("drop-before");
    else body.classList.add("drop-end");
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
    return api.saveLayout({ ...this.layout, lanes }).catch((err) => toast(`Zapis układu: ${err}`));
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

  private async newProject(agent: string): Promise<void> {
    if (this.creatingProject) return; // ⌘N again while the dialog is up
    this.creatingProject = true;
    try {
      await this.createProject(agent);
    } finally {
      this.creatingProject = false;
    }
  }

  private async createProject(agent: string): Promise<void> {
    const parent = this.layout.projectsDir ?? `${this.home}/Documents/projects`;
    const pick = async () => {
      const p = await openDialog({ directory: true, multiple: false, defaultPath: parent });
      return typeof p === "string" ? p : null;
    };
    const req = await newProjectBox(parent, pick, agent);
    if (!req) return;
    try {
      const path = await api.createProject(req.parent, req.name, req.git);
      this.layout.projectsDir = req.parent;
      if (!this.layout.extraFolders.includes(path)) this.layout.extraFolders.push(path);
      this.projectCache = null;
      this.save();
      await this.openTile(keyOf({ path, agent, instance: 0 }));
    } catch (err) {
      toast(`Nowy projekt: ${err}`);
    }
  }

  private projects(): Promise<Project[]> {
    if (!this.projectCache) {
      this.projectCache = api.listProjects(this.layout.extraFolders).catch((err) => {
        toast(`Wykrywanie projektów: ${err}`);
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
      sendNotification({ title: `${tile.name} czeka na ciebie`, body: s.message ?? "Claude potrzebuje odpowiedzi" });
    }
  }

  private renderChrome(): void {
    const states = [...this.tiles.values()].map((t) => t.state);
    const count = (s: string) => states.filter((x) => x === s).length;
    const waiting = count("waiting");
    this.top.innerHTML = `<span class="brand" data-tauri-drag-region>desk</span><span class="dim" data-tauri-drag-region> · ${this.tiles.size} sessions</span>${
      waiting ? `<span class="waiting-pill" data-tauri-drag-region>${waiting} czeka</span>` : ""
    }<span class="spacer" data-tauri-drag-region></span>${usageHtml(this.usage, Date.now())}${this.coffeeHtml()}`;
    this.footer.innerHTML =
      hints().map(([k, label]) => `<span><kbd>${k}</kbd> ${escapeHtml(label)}</span>`).join("") +
      `<span class="spacer"></span><span class="counts"><b>${this.tiles.size}</b> sessions · <span class="c-working">${count("working")} working</span> · <span class="c-waiting">${waiting} waiting</span> · ${count("idle")} idle</span>`;
  }

  // ---- coffee ------------------------------------------------------------

  private async cycleCoffee(): Promise<void> {
    const next: Record<CoffeeLevel, CoffeeLevel> = { off: "awake", awake: "lid", lid: "off" };
    try {
      this.coffee = await api.coffeeSet(next[this.coffee.level]);
    } catch (err) {
      toast(`Kawa: ${err}`);
      if (next[this.coffee.level] === "lid") this.coffee = await api.coffeeSet("off").catch(() => this.coffee);
    }
    this.renderChrome();
  }

  private async restoreSleep(): Promise<void> {
    try {
      this.coffee = await api.coffeeRestore();
    } catch (err) {
      toast(`Usypianie: ${err}`);
    }
    this.renderChrome();
  }

  private onCoffeeStopped(s: CoffeeStatus): void {
    this.coffee = s;
    this.renderChrome();
    if (s.stoppedBecause) {
      const msg = `Kawa wyłączona: ${s.stoppedBecause}`;
      toast(msg);
      sendNotification({ title: "desk", body: msg });
    }
  }

  private coffeeHtml(): string {
    const c = this.coffee;
    const label = { off: "☕", awake: "☕ kawa", lid: "☕ kawa · klapka" }[c.level];
    const title = {
      off: "Kawa wyłączona (⌘K). Kliknij: nie usypiaj komputera, dopóki desk działa.",
      awake:
        PLATFORM === "mac" && !c.helperInstalled
          ? "Kawa: komputer nie zaśnie. Kliknij: także przy zamkniętej klapce (macOS raz zapyta o hasło)."
          : "Kawa: komputer nie zaśnie. Kliknij: także przy zamkniętej klapce.",
      lid: "Kawa z zamkniętą klapką. Wyłącza się sama przy przegrzaniu lub baterii ≤15%. Kliknij: wyłącz.",
    }[c.level];
    const stray =
      c.sleepDisabled && c.level !== "lid"
        ? `<button class="coffee warn" data-coffee="restore" title="Usypianie po zamknięciu klapki jest wyłączone w systemie, np. po restarcie w trakcie kawy. Kliknij, aby przywrócić.">sen wyłączony · przywróć</button>`
        : "";
    return `${stray}<button class="coffee ${c.level}" data-coffee="cycle" title="${shortcuts(title)}">${label}</button>`;
  }

  // ---- lock --------------------------------------------------------------

  private async lock(): Promise<void> {
    try {
      await api.lockDesk();
    } catch (err) {
      toast(`Blokada: ${err}`);
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
    const how = { mac: "Touch ID / hasło", windows: "Windows Hello", linux: "hasło konta" }[PLATFORM];
    const field = PLATFORM === "linux" ? `<input type="password" class="lock-password" placeholder="hasło konta" autocomplete="current-password" />` : "";
    this.lockEl.innerHTML = `<div class="lock-box">🔒 desk zablokowany ${field}<button data-unlock>Odblokuj · ${how} <kbd>↵</kbd></button></div>`;
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
      toast(`Nie odblokowano: ${err}`);
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
    const action = actionFor(e);
    if (!action) return;
    const overlay = document.querySelector(".overlay") !== null;
    if (overlay && !["palette", "new", "settings", "close"].includes(action.type)) return; // e.g. ⌘↵ in the palette
    e.preventDefault();
    e.stopPropagation();
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
        const t = this.tiles.get(this.active ?? "");
        if (!t) {
          toast("Najpierw wybierz kafelek projektu");
          break;
        }
        void this.runInProject(t.path);
        break;
      }
      case "focus":
        this.focusIndex(a.index);
        break;
      case "fullscreen":
        this.toggleFullscreen();
        break;
      case "exitFullscreen":
        if (this.fullscreen) this.toggleFullscreen(this.fullscreen);
        break;
    }
  }
}

/** The lane's project cards, without the one being dragged. */
function droppable(body: HTMLElement): HTMLElement[] {
  return [...body.querySelectorAll<HTMLElement>(":scope > .card:not(.dragging):not(:has(.tile.dragging))")];
}

/** Index among the lane's cards before which a drop at `y` lands (ignoring the dragged one). */
function dropIndex(body: HTMLElement, y: number): number {
  const tiles = droppable(body);
  const i = tiles.findIndex((t) => {
    const r = t.getBoundingClientRect();
    return y < r.top + r.height / 2;
  });
  return i < 0 ? tiles.length : i;
}

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
