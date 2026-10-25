// ⌘, options: lane count, notifications, which projects start with the app,
// hand-added folders.
import type { AgentInfo, Layout, Project } from "./api";
import { rows, shortPath } from "./projects";
import { escapeHtml } from "./tile";

export interface SettingsContext {
  home: () => string;
  layout: () => Layout;
  projects: () => Promise<Project[]>;
  setLaneCount: (n: number) => void;
  setNotifications: (on: boolean) => void;
  toggleAutostart: (path: string) => void;
  addFolder: () => Promise<void>;
  removeFolder: (path: string) => void;
  agents: () => AgentInfo[];
  addAgent: (id: string, command: string) => Promise<void>;
  removeAgent: (id: string) => Promise<void>;
}

export class Settings {
  private el: HTMLElement | null = null;
  private projects: Project[] = [];
  private filter = "";

  constructor(private ctx: SettingsContext) {}

  get isOpen(): boolean {
    return this.el !== null;
  }

  async toggle(): Promise<void> {
    if (this.el) return this.close();
    this.el = document.createElement("div");
    this.el.className = "overlay";
    this.el.innerHTML = `<div class="dialog settings"></div>`;
    this.el.addEventListener("mousedown", (e) => {
      if (e.target === this.el) this.close();
    });
    this.el.addEventListener("keydown", (e) => {
      if (e.key === "Escape") this.close();
    });
    document.body.appendChild(this.el);
    this.render();
    this.projects = await this.ctx.projects();
    this.render();
  }

  close(): void {
    this.el?.remove();
    this.el = null;
  }

  async reload(): Promise<void> {
    this.projects = await this.ctx.projects();
    this.render();
  }

  render(): void {
    const box = this.el?.querySelector<HTMLElement>(".dialog");
    if (!box) return;
    const layout = this.ctx.layout();
    const auto = new Set(layout.autostart);
    const list = rows(this.projects, this.filter);
    // Autostart entries first so the current choice is visible without scrolling.
    list.sort((a, b) => Number(auto.has(b.project.path)) - Number(auto.has(a.project.path)));

    box.innerHTML = `
      <h2>Opcje</h2>
      <label class="line">Kolumny <input type="number" min="1" max="8" value="${layout.lanes.length}" data-k="lanes" /></label>
      <label class="line"><input type="checkbox" data-k="notify" ${layout.notifications ? "checked" : ""}/> Powiadomienie macOS, gdy sesja czeka na ciebie</label>
      <h3>Start z aplikacją <span class="dim">(${auto.size})</span></h3>
      <input type="text" class="filter" placeholder="Filtruj projekty…" value="${escapeHtml(this.filter)}" data-k="filter" spellcheck="false" />
      <div class="checks">${
        list.length
          ? list
              .map(
                (r) => `<label class="${r.parent ? "child" : ""}"><input type="checkbox" data-auto="${escapeHtml(r.project.path)}" ${
                  auto.has(r.project.path) ? "checked" : ""
                }/> <b>${escapeHtml(r.project.name)}</b> <span class="dim">${escapeHtml(shortPath(r.project.path, this.ctx.home()))}</span></label>`,
              )
              .join("")
          : `<div class="dim">Szukam projektów…</div>`
      }</div>
      <h3>Agenci <span class="dim">(Tab w palecie wybiera agenta)</span></h3>
      <div class="agents">${this.ctx
        .agents()
        .map(
          (a) => `<div class="agent-row"><b>${escapeHtml(a.id)}</b> <span class="dim">${escapeHtml(a.command || "login shell")}</span> ${
            a.installed ? `<span class="tag open">jest</span>` : `<span class="tag">brak w PATH</span>`
          }${a.builtin ? "" : ` <button data-rm-agent="${escapeHtml(a.id)}">usuń</button>`}</div>`,
        )
        .join("")}
        <div class="line"><input type="text" data-k="agent-id" placeholder="nazwa, np. amp" spellcheck="false" /><input type="text" data-k="agent-cmd" placeholder="komenda, np. amp --dangerously-allow-all" spellcheck="false" /><button data-k="agent-add">+ Dodaj agenta</button></div>
      </div>
      <h3>Dodane foldery</h3>
      <div class="folders">${layout.extraFolders
        .map(
          (f) => `<div class="folder"><span>${escapeHtml(shortPath(f, this.ctx.home()))}</span><button data-rm="${escapeHtml(f)}">usuń</button></div>`,
        )
        .join("")}<button data-k="add">+ Dodaj folder…</button></div>
      <p class="dim small">Ustawienia: ~/Library/Application Support/desk/layout.json · sesje: tmux -L desk ls</p>`;

    const filter = box.querySelector<HTMLInputElement>('[data-k="filter"]')!;
    filter.addEventListener("input", () => {
      this.filter = filter.value;
      const pos = filter.selectionStart;
      this.render();
      const again = this.el?.querySelector<HTMLInputElement>('[data-k="filter"]');
      again?.focus();
      again?.setSelectionRange(pos, pos);
    });
    box.querySelector<HTMLInputElement>('[data-k="lanes"]')!.addEventListener("change", (e) => {
      this.ctx.setLaneCount(Number((e.target as HTMLInputElement).value));
      this.render();
    });
    box.querySelector<HTMLInputElement>('[data-k="notify"]')!.addEventListener("change", (e) => {
      this.ctx.setNotifications((e.target as HTMLInputElement).checked);
    });
    box.querySelectorAll<HTMLInputElement>("[data-auto]").forEach((cb) =>
      cb.addEventListener("change", () => this.ctx.toggleAutostart(cb.dataset.auto!)),
    );
    box.querySelectorAll<HTMLButtonElement>("[data-rm]").forEach((b) =>
      b.addEventListener("click", () => {
        this.ctx.removeFolder(b.dataset.rm!);
        void this.reload();
      }),
    );
    box.querySelectorAll<HTMLButtonElement>("[data-rm-agent]").forEach((b) =>
      b.addEventListener("click", async () => {
        await this.ctx.removeAgent(b.dataset.rmAgent!);
        this.render();
      }),
    );
    box.querySelector<HTMLButtonElement>('[data-k="agent-add"]')!.addEventListener("click", async () => {
      const id = box.querySelector<HTMLInputElement>('[data-k="agent-id"]')!.value.trim().toLowerCase();
      const cmd = box.querySelector<HTMLInputElement>('[data-k="agent-cmd"]')!.value.trim();
      if (!/^[a-z0-9][a-z0-9_-]*$/.test(id) || !cmd) return;
      await this.ctx.addAgent(id, cmd);
      this.render();
    });
    box.querySelector<HTMLButtonElement>('[data-k="add"]')!.addEventListener("click", async () => {
      await this.ctx.addFolder();
      await this.reload();
    });
  }
}
