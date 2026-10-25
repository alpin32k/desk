// ⌘P project palette: search, Tab picks the agent, Enter opens a tile,
// ⌘Enter toggles autostart, ⌘⌫ kills a running session.
import type { AgentInfo, Project } from "./api";
import { ago, rows, shortPath, type Row } from "./projects";
import { escapeHtml } from "./tile";

export interface PaletteContext {
  home: () => string;
  projects: () => Promise<Project[]>;
  agents: () => AgentInfo[];
  isOpen: (path: string, agent: string) => boolean;
  isRunning: (path: string, agent: string) => boolean;
  isAutostart: (path: string, agent: string) => boolean;
  color: (path: string) => string;
  open: (path: string, agent: string) => void;
  toggleAutostart: (path: string, agent: string) => void;
  kill: (path: string, agent: string) => void;
  addFolder: () => void;
  newProject: (agent: string) => void;
  scratch: (agent: string) => void;
}

const MAX_ROWS = 200;

export class Palette {
  private el: HTMLElement | null = null;
  private input!: HTMLInputElement;
  private list!: HTMLElement;
  private all: Project[] = [];
  private shown: Row[] = [];
  private selected = 0;
  private mode: "all" | "new" = "all";
  private agent = "claude";

  constructor(private ctx: PaletteContext) {}

  get isOpen(): boolean {
    return this.el !== null;
  }

  async show(mode: "all" | "new"): Promise<void> {
    if (this.el) {
      if (this.mode === mode) return this.close();
      this.mode = mode;
      this.refresh();
      return;
    }
    this.mode = mode;
    this.el = document.createElement("div");
    this.el.className = "overlay";
    this.el.innerHTML = `
      <div class="dialog palette">
        <input type="text" spellcheck="false" autocomplete="off" />
        <div class="list"></div>
        <footer><span class="agent-pick"></span><kbd>↵</kbd> otwórz <kbd>⌘↵</kbd> start z aplikacją <kbd>⌘⌫</kbd> zabij sesję <kbd>esc</kbd> zamknij</footer>
      </div>`;
    this.input = this.el.querySelector("input")!;
    this.list = this.el.querySelector(".list")!;
    this.input.addEventListener("input", () => {
      this.selected = 0;
      this.refresh();
    });
    this.input.addEventListener("keydown", (e) => this.onKey(e));
    this.el.addEventListener("mousedown", (e) => {
      if (e.target === this.el) this.close();
    });
    this.list.addEventListener("click", (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>("[data-i]");
      if (!row) return;
      this.selected = Number(row.dataset.i);
      this.activate();
    });
    document.body.appendChild(this.el);
    this.input.focus();
    this.list.innerHTML = `<div class="empty">Szukam projektów…</div>`;
    this.all = await this.ctx.projects();
    this.refresh();
  }

  close(): void {
    this.el?.remove();
    this.el = null;
  }

  /** Re-render after open/running/autostart state changed. */
  refresh(): void {
    if (!this.el) return;
    const q = this.input.value;
    this.input.placeholder = this.mode === "new" ? "Nowa sesja: wybierz projekt…" : "Projekt…";
    let r = rows(this.all, q);
    if (this.mode === "new") r = r.filter((x) => !this.ctx.isOpen(x.project.path, this.agent));
    this.shown = r.slice(0, MAX_ROWS);
    this.selected = Math.min(this.selected, this.shown.length + 2); // three action rows at the end
    const items = this.shown.map((row, i) => this.rowHtml(row, i));
    const n = this.shown.length;
    items.push(
      `<div class="row action ${this.selected === n ? "sel" : ""}" data-i="${n}">+ Nowy projekt…</div>`,
      `<div class="row action ${this.selected === n + 1 ? "sel" : ""}" data-i="${n + 1}">+ Dodaj istniejący folder…</div>`,
      `<div class="row action ${this.selected === n + 2 ? "sel" : ""}" data-i="${n + 2}">+ Luźne pytanie, czysta sesja bez projektu (⌘L)</div>`,
    );
    const agents = this.ctx.agents().filter((a) => a.installed);
    if (!agents.some((a) => a.id === this.agent)) this.agent = agents[0]?.id ?? "claude";
    this.el.querySelector(".agent-pick")!.innerHTML = `<kbd>⇥</kbd> agent: ${agents
      .map((a) => `<span class="${a.id === this.agent ? "on" : ""}">${escapeHtml(a.id)}</span>`)
      .join(" ")}`;
    this.list.innerHTML = items.join("");
    this.list.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
  }

  private rowHtml(row: Row, i: number): string {
    const p = row.project;
    const open = this.ctx.isOpen(p.path, this.agent);
    const tags = [
      open ? `<span class="tag open">otwarty</span>` : "",
      !open && this.ctx.isRunning(p.path, this.agent) ? `<span class="tag run">w tle</span>` : "",
      this.ctx.isAutostart(p.path, this.agent) ? `<span class="tag auto">★ start</span>` : "",
    ].join("");
    const name = row.parent && !this.input.value.trim() ? `<span class="wt">↳</span> ${escapeHtml(p.name)}` : escapeHtml(p.name);
    const parent = row.parent ? `<span class="dim"> · worktree ${escapeHtml(row.parent.name)}</span>` : "";
    return `<div class="row ${i === this.selected ? "sel" : ""} ${row.parent ? "child" : ""}" data-i="${i}">
      <span class="dot" style="background:${this.ctx.color(p.path)}"></span>
      <span class="name">${name}${parent}</span>
      <span class="path">${escapeHtml(shortPath(p.path, this.ctx.home()))}</span>
      ${tags}<span class="age">${ago(p.lastActiveMs)}</span></div>`;
  }

  private onKey(e: KeyboardEvent): void {
    const count = this.shown.length + 3;
    if (e.key === "Tab") {
      const ids = this.ctx.agents().filter((a) => a.installed).map((a) => a.id);
      const i = ids.indexOf(this.agent);
      this.agent = ids[(i + (e.shiftKey ? ids.length - 1 : 1)) % ids.length] ?? "claude";
    } else if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
      this.selected = (this.selected + 1) % count;
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
      this.selected = (this.selected - 1 + count) % count;
    } else if (e.key === "Enter" && e.metaKey) {
      const row = this.shown[this.selected];
      if (row) this.ctx.toggleAutostart(row.project.path, this.agent);
    } else if (e.key === "Backspace" && e.metaKey) {
      const row = this.shown[this.selected];
      if (row && this.ctx.isRunning(row.project.path, this.agent)) this.ctx.kill(row.project.path, this.agent);
    } else if (e.key === "Enter") {
      this.activate();
      return;
    } else if (e.key === "Escape") {
      this.close();
      return;
    } else {
      return;
    }
    e.preventDefault();
    this.refresh();
  }

  private activate(): void {
    const row = this.shown[this.selected];
    const action = this.selected - this.shown.length; // 0 new project, 1 add folder, 2 loose question
    this.close();
    if (row) this.ctx.open(row.project.path, this.agent);
    else if (action === 0) this.ctx.newProject(this.agent);
    else if (action === 1) this.ctx.addFolder();
    else this.ctx.scratch(this.agent);
  }
}
