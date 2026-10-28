// ⌘P project palette: search, Tab picks the agent, Enter opens a tile,
// ⌘Enter toggles autostart, ⌘⌫ kills a running session.
import type { AgentInfo, GithubRepo, Project } from "./api";
import { matchRepos, repoName } from "./github";
import { ago, rows, shortPath, type Row } from "./projects";
import { escapeHtml } from "./tile";
import { keysIn } from "./keys";
import { PLATFORM, shortcut } from "./platform";

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
  /** `name`: what was typed in the palette, as the suggested name. */
  newProject: (agent: string, name?: string) => void;
  chat: (agent: string) => void;
  /** GitHub repos to offer when nothing local is called that. */
  repos: () => Promise<GithubRepo[]>;
  /** Where a repo picked here gets cloned. */
  projectsDir: () => string;
  cloneRepo: (repo: GithubRepo, agent: string) => void;
}

const MAX_ROWS = 200;

export class Palette {
  private el: HTMLElement | null = null;
  private input!: HTMLInputElement;
  private list!: HTMLElement;
  private all: Project[] = [];
  private shown: Row[] = [];
  private repos: GithubRepo[] = [];
  /** GitHub rows, after the local ones. */
  private remote: GithubRepo[] = [];
  private selected = 0;
  private agent = "claude";

  constructor(private ctx: PaletteContext) {}

  get isOpen(): boolean {
    return this.el !== null;
  }

  /** Toggles: a second ⌘P closes the palette. */
  async show(): Promise<void> {
    if (this.el) return this.close();
    this.el = document.createElement("div");
    this.el.className = "overlay";
    this.el.innerHTML = `
      <div class="dialog palette">
        <input type="text" spellcheck="false" autocomplete="off" />
        <div class="list"></div>
        <footer><span class="agent-pick"></span><kbd>↵</kbd> otwórz <kbd>${shortcut("⌘↵")}</kbd> start z aplikacją <kbd>${shortcut("⌘⌫")}</kbd> zabij sesję <kbd>esc</kbd> zamknij</footer>
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
    // No gh or no login: just no GitHub rows.
    this.ctx.repos().then(
      (r) => {
        this.repos = r;
        if (this.all.length) this.refresh();
      },
      () => {},
    );
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
    this.input.placeholder = "Projekt…";
    const r = rows(this.all, q);
    this.shown = r.slice(0, MAX_ROWS);
    const local = new Set(this.all.map((p) => p.name.toLowerCase()));
    this.remote = matchRepos(this.repos, q, 5).filter((repo) => !local.has(repoName(repo).toLowerCase()));
    const n = this.shown.length + this.remote.length;
    this.selected = Math.min(this.selected, n + 2); // three action rows at the end
    const items = this.shown.map((row, i) => this.rowHtml(row, i));
    items.push(...this.remote.map((repo, i) => this.repoHtml(repo, this.shown.length + i)));
    items.push(
      `<div class="row action ${this.selected === n ? "sel" : ""}" data-i="${n}">+ Nowy projekt…</div>`,
      `<div class="row action ${this.selected === n + 1 ? "sel" : ""}" data-i="${n + 1}">+ Dodaj istniejący folder…</div>`,
      `<div class="row action ${this.selected === n + 2 ? "sel" : ""}" data-i="${n + 2}">+ Rozmowa: zwykły czat o czymkolwiek, bez projektu (${keysIn("⌘R")})</div>`,
    );
    const agents = this.ctx.agents().filter(usable);
    if (!agents.some((a) => a.id === this.agent)) this.agent = agents[0]?.id ?? "claude";
    this.el.querySelector(".agent-pick")!.innerHTML = `<kbd>⇥</kbd> agent: ${agents
      .map(
        (a) =>
          `<span class="${a.id === this.agent ? "on" : ""}"${a.installed ? "" : ` title="zainstaluje się przy otwarciu"`}>${escapeHtml(a.id)}${a.installed ? "" : " ↓"}</span>`,
      )
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

  private repoHtml(repo: GithubRepo, i: number): string {
    const target = `${shortPath(this.ctx.projectsDir(), this.ctx.home())}/${repoName(repo)}`;
    return `<div class="row ${i === this.selected ? "sel" : ""}" data-i="${i}">
      <span class="dot gh"></span>
      <span class="name">${escapeHtml(repo.fullName)}</span>
      <span class="path">sklonuj do ${escapeHtml(target)}</span>
      ${repo.private ? `<span class="tag">prywatne</span>` : ""}<span class="tag gh">GitHub</span></div>`;
  }

  private onKey(e: KeyboardEvent): void {
    const count = this.shown.length + this.remote.length + 3;
    if (e.key === "Tab") {
      const ids = this.ctx.agents().filter(usable).map((a) => a.id);
      const i = ids.indexOf(this.agent);
      this.agent = ids[(i + (e.shiftKey ? ids.length - 1 : 1)) % ids.length] ?? "claude";
    } else if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
      this.selected = (this.selected + 1) % count;
    } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
      this.selected = (this.selected - 1 + count) % count;
    } else if (e.key === "Enter" && (PLATFORM === "mac" ? e.metaKey : e.ctrlKey)) {
      const row = this.shown[this.selected];
      if (row) this.ctx.toggleAutostart(row.project.path, this.agent);
    } else if (e.key === "Backspace" && (PLATFORM === "mac" ? e.metaKey : e.ctrlKey)) {
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
    const repo = this.remote[this.selected - this.shown.length];
    const action = this.selected - this.shown.length - this.remote.length; // 0 new project, 1 add folder, 2 loose question
    const query = this.input.value.trim();
    this.close();
    if (row) this.ctx.open(row.project.path, this.agent);
    else if (repo) this.ctx.cloneRepo(repo, this.agent);
    else if (action === 0) this.ctx.newProject(this.agent, query || undefined);
    else if (action === 1) this.ctx.addFolder();
    else this.ctx.chat(this.agent);
  }
}

/** Installed, or a built-in desk installs on first use. */
function usable(a: AgentInfo): boolean {
  return a.installed || a.installable;
}
