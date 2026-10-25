// Main controller: lanes of tiles, shortcuts, session state, persistence.
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { api, type Layout, type Project, type PtyExit, type SessionState } from "./api";
import { actionFor, HINTS, type Action } from "./keys";
import * as L from "./lanes";
import { Palette } from "./palette";
import { Settings } from "./settings";
import { displayState, escapeHtml, Tile, type TileAction } from "./tile";
import { confirmBox, toast } from "./ui";

const POLL_MS = 3000;

export class App {
  private layout!: Layout;
  private home = "";
  private tiles = new Map<string, Tile>(); // by project path
  private hook = new Map<string, SessionState>(); // by tmux slug
  private commands = new Map<string, string | null>(); // by project path
  private running = new Set<string>(); // project paths with a tmux session
  private projectCache: Promise<Project[]> | null = null;
  private active: string | null = null;
  private fullscreen: string | null = null;

  private grid: HTMLElement;
  private top: HTMLElement;
  private footer: HTMLElement;
  private palette: Palette;
  private settings: Settings;

  constructor(root: HTMLElement) {
    root.innerHTML = `
      <div class="top" data-tauri-drag-region></div>
      <main class="grid"></main>
      <footer class="bottom"></footer>`;
    this.top = root.querySelector(".top")!;
    this.grid = root.querySelector(".grid")!;
    this.footer = root.querySelector(".bottom")!;

    this.palette = new Palette({
      home: () => this.home,
      projects: () => this.projects(),
      isOpen: (p) => this.tiles.has(p),
      isRunning: (p) => this.running.has(p),
      isAutostart: (p) => this.layout.autostart.includes(p),
      open: (p) => void this.openTile(p),
      toggleAutostart: (p) => this.toggleAutostart(p),
      kill: (p) => void this.killSession(p),
      addFolder: () => void this.addFolder(),
    });
    this.settings = new Settings({
      home: () => this.home,
      layout: () => this.layout,
      projects: () => this.projects(),
      setLaneCount: (n) => {
        this.layout.lanes = L.setLaneCount(this.layout.lanes, n);
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
    });
  }

  async start(): Promise<void> {
    const boot = await api.bootstrap();
    this.home = boot.home;
    for (const s of boot.states) this.hook.set(s.slug, s);
    for (const s of boot.sessions) this.running.add(s.path);

    this.layout = { ...boot.layout, lanes: L.restore(boot.layout.lanes, boot.layout.autostart, boot.layout.lanes.length) };

    window.addEventListener("keydown", (e) => this.onKey(e), true);
    window.addEventListener("resize", () => this.fitAll());
    await listen<SessionState>("session-state", (e) => this.onHookState(e.payload));
    await listen<PtyExit>("pty-exit", (e) => void this.onPtyExit(e.payload));

    // Reattach every tile from the saved layout (starting sessions tmux lost) and the autostart ones.
    const paths = L.flatten(this.layout.lanes);
    const opened = await Promise.all(paths.map((p) => this.createTile(p)));
    paths.forEach((p, i) => {
      if (!opened[i]) this.layout.lanes = L.remove(this.layout.lanes, p);
    });
    this.render();
    await Promise.all([...this.tiles.values()].map((t) => t.start().catch((err) => toast(`${t.name}: ${err}`))));
    this.focusIndex(0);
    this.save();
    void this.poll();
    setInterval(() => void this.poll(), POLL_MS);
    if (this.layout.notifications) void ensureNotificationPermission();
    if (!this.tiles.size) void this.palette.show("all");
  }

  // ---- tiles -------------------------------------------------------------

  private async createTile(path: string): Promise<Tile | null> {
    try {
      const slug = await api.openSession(path);
      this.running.add(path);
      const tile = new Tile(path, slug, (t, a) => void this.onTileAction(t, a));
      this.tiles.set(path, tile);
      return tile;
    } catch (err) {
      toast(`Nie udało się otworzyć ${path}: ${err}`);
      return null;
    }
  }

  async openTile(path: string): Promise<void> {
    const existing = this.tiles.get(path);
    if (existing) return this.setActive(path, true);
    const tile = await this.createTile(path);
    if (!tile) return;
    this.layout.lanes = L.add(this.layout.lanes, path);
    this.render();
    try {
      await tile.start();
    } catch (err) {
      toast(`${tile.name}: ${err}`);
    }
    this.setActive(path, true);
    this.save();
    void this.poll();
  }

  /** Close the tile; the tmux session keeps running in the background. */
  closeTile(path: string): void {
    const tile = this.tiles.get(path);
    if (!tile) return;
    const order = L.flatten(this.layout.lanes);
    const at = order.indexOf(path);
    tile.dispose();
    this.tiles.delete(path);
    this.layout.lanes = L.remove(this.layout.lanes, path);
    if (this.fullscreen === path) this.fullscreen = null;
    this.changed();
    const next = order.filter((p) => p !== path)[Math.max(0, at - 1)];
    if (next) this.setActive(next, true);
  }

  async killSession(path: string): Promise<void> {
    const name = path.split("/").pop();
    if (!(await confirmBox(`Zabić sesję Claude w „${name}”? Bieżąca rozmowa zostanie przerwana.`, "Zabij"))) return;
    const tile = this.tiles.get(path);
    if (tile) this.closeTile(path);
    const slug = tile?.slug ?? (await api.listSessions()).find((s) => s.path === path)?.slug;
    if (slug) await api.killSession(slug).catch((err) => toast(String(err)));
    this.running.delete(path);
    this.palette.refresh();
  }

  private async onTileAction(tile: Tile, action: TileAction): Promise<void> {
    if (action === "focus") return this.setActive(tile.path, false);
    if (action === "close") return this.closeTile(tile.path);
    if (action === "kill") return this.killSession(tile.path);
    if (action === "fullscreen") return this.toggleFullscreen(tile.path);
  }

  private async onPtyExit(e: PtyExit): Promise<void> {
    const tile = [...this.tiles.values()].find((t) => t.ptyId === e.id);
    if (!tile) return;
    const alive = (await api.listSessions()).some((s) => s.slug === e.slug);
    if (alive) {
      await tile.attach(); // detached from outside (e.g. tmux detach-client): attach again
    } else {
      this.running.delete(tile.path);
      this.closeTile(tile.path);
      toast(`Sesja ${tile.name} zakończona`);
    }
  }

  private setActive(path: string, focus: boolean): void {
    if (this.active !== path) {
      this.tiles.get(this.active ?? "")?.setActive(false);
      this.active = path;
      this.tiles.get(path)?.setActive(true);
      if (this.fullscreen && this.fullscreen !== path) this.toggleFullscreen(path);
    }
    if (focus) this.tiles.get(path)?.focus();
  }

  private focusIndex(i: number): void {
    const path = L.flatten(this.layout.lanes)[i];
    if (path) this.setActive(path, true);
  }

  private toggleFullscreen(path?: string): void {
    const target = path ?? this.active;
    this.fullscreen = !target || this.fullscreen === target ? null : target;
    this.grid.classList.toggle("has-full", this.fullscreen !== null);
    for (const t of this.tiles.values()) t.el.classList.toggle("full", t.path === this.fullscreen);
    if (target) this.setActive(target, true);
    this.fitAll();
  }

  // ---- layout ------------------------------------------------------------

  private render(): void {
    const lanes = this.layout.lanes;
    while (this.grid.children.length > lanes.length) this.grid.lastElementChild!.remove();
    while (this.grid.children.length < lanes.length) this.grid.appendChild(this.laneEl());
    lanes.forEach((paths, i) => {
      const lane = this.grid.children[i] as HTMLElement;
      lane.dataset.lane = String(i);
      lane.querySelector(".lane-head")!.innerHTML =
        `LANE ${i + 1} <span class="dim">${paths.length || ""}</span>`;
      const body = lane.querySelector<HTMLElement>(".lane-body")!;
      for (const p of paths) {
        const t = this.tiles.get(p);
        if (t && t.el.parentElement !== body) body.appendChild(t.el);
      }
      // Order inside the lane.
      paths.forEach((p) => {
        const t = this.tiles.get(p);
        if (t) body.appendChild(t.el);
      });
      body.classList.toggle("empty", paths.length === 0);
    });
    L.flatten(lanes).forEach((p, i) => this.tiles.get(p)?.update({ index: i + 1 }));
    this.renderChrome();
    this.fitAll();
  }

  private laneEl(): HTMLElement {
    const lane = document.createElement("div");
    lane.className = "lane";
    lane.innerHTML = `<div class="lane-head"></div><div class="lane-body"><div class="hint">⌘K — otwórz projekt</div></div>`;
    const body = lane.querySelector<HTMLElement>(".lane-body")!;
    body.addEventListener("dragover", (e) => {
      if (!e.dataTransfer?.types.includes("text/x-desk-path")) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = "move";
      this.markDrop(body, dropIndex(body, e.clientY));
    });
    body.addEventListener("dragleave", (e) => {
      if (!body.contains(e.relatedTarget as Node)) this.markDrop(null, 0);
    });
    body.addEventListener("drop", (e) => {
      const path = e.dataTransfer?.getData("text/x-desk-path");
      this.markDrop(null, 0);
      if (!path) return;
      e.preventDefault();
      const laneIdx = Number(lane.dataset.lane);
      const index = dropIndex(body, e.clientY);
      this.layout.lanes = L.move(this.layout.lanes, path, laneIdx, index);
      this.changed();
      this.setActive(path, true);
    });
    return lane;
  }

  private markDrop(body: HTMLElement | null, index: number): void {
    this.grid.querySelectorAll(".drop-before, .drop-end").forEach((el) => el.classList.remove("drop-before", "drop-end"));
    if (!body) return;
    const tiles = [...body.querySelectorAll<HTMLElement>(":scope > .tile:not(.dragging)")];
    if (tiles[index]) tiles[index].classList.add("drop-before");
    else body.classList.add("drop-end");
  }

  private changed(): void {
    this.render();
    this.save();
  }

  private saveTimer: number | undefined;
  private save(): void {
    clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => {
      api.saveLayout(this.layout).catch((err) => toast(`Zapis układu: ${err}`));
    }, 150);
  }

  private fitAll(): void {
    for (const t of this.tiles.values()) t.queueFit();
  }

  private toggleAutostart(path: string): void {
    const a = this.layout.autostart;
    this.layout.autostart = a.includes(path) ? a.filter((p) => p !== path) : [...a, path];
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
    const paths = [...this.tiles.keys()];
    const [infos, sessions] = await Promise.all([api.tileInfo(paths), api.listSessions()]);
    this.running = new Set(sessions.map((s) => s.path));
    for (const info of infos) {
      this.commands.set(info.path, info.command);
      const tile = this.tiles.get(info.path);
      tile?.update({ branch: info.branch, state: displayState(this.hook.get(tile.slug)?.state, info.command) });
    }
    this.renderChrome();
  }

  private onHookState(s: SessionState): void {
    const before = this.hook.get(s.slug)?.state;
    this.hook.set(s.slug, s);
    const tile = [...this.tiles.values()].find((t) => t.slug === s.slug);
    if (!tile) return;
    tile.update({ state: displayState(s.state, this.commands.get(tile.path)) });
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
    }`;
    this.footer.innerHTML =
      HINTS.map(([k, label]) => `<span><kbd>${k}</kbd> ${escapeHtml(label)}</span>`).join("") +
      `<span class="spacer"></span><span class="counts"><b>${this.tiles.size}</b> sessions · <span class="c-working">${count("working")} working</span> · <span class="c-waiting">${waiting} waiting</span> · ${count("idle")} idle</span>`;
  }

  // ---- keys --------------------------------------------------------------

  private onKey(e: KeyboardEvent): void {
    const action = actionFor(e);
    if (!action) return;
    e.preventDefault();
    e.stopPropagation();
    this.run(action);
  }

  private run(a: Action): void {
    switch (a.type) {
      case "palette":
        this.settings.close();
        void this.palette.show("all");
        break;
      case "new":
        this.settings.close();
        void this.palette.show("new");
        break;
      case "settings":
        this.palette.close();
        void this.settings.toggle();
        break;
      case "close":
        if (this.palette.isOpen) this.palette.close();
        else if (this.settings.isOpen) this.settings.close();
        else if (this.active) this.closeTile(this.active);
        break;
      case "kill":
        if (this.active) void this.killSession(this.active);
        break;
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

/** Index among the lane's tiles before which a drop at `y` lands (ignoring the dragged tile). */
function dropIndex(body: HTMLElement, y: number): number {
  const tiles = [...body.querySelectorAll<HTMLElement>(":scope > .tile:not(.dragging)")];
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
