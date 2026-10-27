// ⌘O options: lane count, notifications, which projects start with the app,
// hand-added folders.
import { getVersion } from "@tauri-apps/api/app";
import { DEFAULT_THEME, THEMES } from "./themes";
import type { AgentInfo, CoffeeStatus, Layout, Project } from "./api";
import { rows, shortPath } from "./projects";
import { escapeHtml } from "./tile";
import { acceleratorFor, DEFAULT_HOTKEY, hotkeyLabel } from "./hotkey";
import { acceleratorsOf, BINDINGS, clashWith, FIXED, GROUPS, keysIn, keyText } from "./keys";
import { shortcut } from "./platform";
import { toast } from "./ui";

export interface SettingsContext {
  home: () => string;
  layout: () => Layout;
  projects: () => Promise<Project[]>;
  setLaneCount: (n: number) => void;
  setAutoLayout: (on: boolean) => void;
  setTheme: (id: string) => void;
  setBrowser: (id: string) => void;
  setFontSize: (px: number) => void;
  setDropdown: (
    change: Partial<Pick<Layout, "dropdown" | "dropdownHeight" | "dropdownAutohide" | "dropdownHotkey" | "dropdownAnimation">>,
  ) => Promise<void>;
  /** A desk shortcut set in Options, or back to its default (null). */
  setKey: (id: string, accelerator: string | null) => void;
  /** Ask GitHub for a newer desk now. */
  checkUpdate: () => Promise<void>;
  setNotifications: (on: boolean) => void;
  setTmuxMenu: (on: boolean) => void;
  toggleAutostart: (path: string) => void;
  addFolder: () => Promise<void>;
  removeFolder: (path: string) => void;
  coffee: () => CoffeeStatus;
  uninstallCoffee: () => Promise<void>;
  agents: () => AgentInfo[];
  addAgent: (id: string, command: string) => Promise<void>;
  removeAgent: (id: string) => Promise<void>;
}

export class Settings {
  private el: HTMLElement | null = null;
  private projects: Project[] = [];
  private filter = "";
  /** Waiting for a new shortcut: "dropdown" or a binding id (keys.ts). */
  private recording: string | null = null;
  private tab: "general" | "dropdown" | "keys" = "general";

  constructor(private ctx: SettingsContext) {}

  get isOpen(): boolean {
    return this.el !== null;
  }

  /** Keys go to the shortcut field, not to desk's shortcuts. */
  get isRecording(): boolean {
    return this.recording !== null;
  }

  async toggle(): Promise<void> {
    if (this.el) return this.close();
    this.el = document.createElement("div");
    this.el.className = "overlay";
    this.el.innerHTML = `<div class="dialog settings"></div>`;
    this.el.addEventListener("mousedown", (e) => {
      if (e.target === this.el) this.close();
    });
    // On the window: the focus usually stays in the terminal behind.
    window.addEventListener("keydown", this.onKey, true);
    document.body.appendChild(this.el);
    this.render();
    this.projects = await this.ctx.projects();
    this.render();
  }

  close(): void {
    window.removeEventListener("keydown", this.onKey, true);
    this.recording = null;
    this.el?.remove();
    this.el = null;
  }

  private onKey = (e: KeyboardEvent): void => {
    if (this.recording) {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
        this.recording = null;
        this.render();
        return;
      }
      const accelerator = acceleratorFor(e);
      if (!accelerator) return; // a modifier alone: wait for the key
      const id = this.recording;
      this.recording = null;
      this.record(id, accelerator);
      return;
    }
    if (e.key !== "Escape" || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return;
    e.preventDefault();
    e.stopPropagation();
    this.close();
  };

  /** A new shortcut was pressed for `id`: taken keys are refused. */
  private record(id: string, accelerator: string): void {
    const layout = this.ctx.layout();
    const dropdown = layout.dropdown ? layout.dropdownHotkey || DEFAULT_HOTKEY() : "";
    const clash = id !== "dropdown" && accelerator === dropdown ? "dropdown" : clashWith(id, accelerator);
    if (clash) {
      toast(`${hotkeyLabel(accelerator)} jest już zajęty: ${clash}`);
      this.render();
      return;
    }
    if (id === "dropdown") {
      void this.ctx.setDropdown({ dropdownHotkey: accelerator }).then(() => this.render());
      return;
    }
    const b = BINDINGS.find((x) => x.id === id)!;
    // Its default again: nothing to keep in layout.keys.
    this.ctx.setKey(id, acceleratorsOf(b).length === 1 && acceleratorsOf(b)[0] === accelerator ? null : accelerator);
    this.render();
  }

  private keyButton(id: string, label: string, disabled = ""): string {
    const on = this.recording === id;
    return `<button class="key ${on ? "recording" : ""}" data-record="${escapeHtml(id)}" ${disabled}>${
      on ? "naciśnij…" : escapeHtml(label)
    }</button>`;
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

    const hotkey = hotkeyLabel(layout.dropdownHotkey || DEFAULT_HOTKEY());
    const off = layout.dropdown ? "" : "disabled";
    // Every tab is in the page (the hidden ones too), so every field keeps its listener.
    box.innerHTML = `
      <div class="settings-head"><h2>Opcje</h2><div class="tabs">${[
        ["general", "Ogólne"],
        ["dropdown", "Dropdown"],
        ["keys", "Skróty"],
      ]
        .map(([id, label]) => `<button class="tab ${this.tab === id ? "sel" : ""}" data-tab="${id}">${label}</button>`)
        .join("")}</div></div>
      <div class="pane" data-pane="general" ${this.tab === "general" ? "" : "hidden"}>
      <label class="line"><input type="checkbox" data-k="auto-layout" ${layout.autoLayout ? "checked" : ""}/> Układ automatyczny <span class="dim">(kolumny według otwartych projektów, sesji i rozmiaru okna)</span></label>
      <label class="line">Kolumny <input type="number" min="1" max="8" value="${layout.lanes.length}" data-k="lanes" ${layout.autoLayout ? "disabled" : ""}/></label>
      <label class="line"><input type="checkbox" data-k="notify" ${layout.notifications ? "checked" : ""}/> Powiadomienie macOS, gdy sesja czeka na ciebie</label>
      <label class="line"><input type="checkbox" data-k="tmux-menu" ${layout.tmuxMenu ? "checked" : ""}/> Menu tmuxa pod prawym klikiem w terminalu <span class="dim">(podział, zamiana, zamknięcie panelu)</span></label>
      <label class="line">Czcionka terminali <input type="number" min="8" max="24" step="0.5" value="${layout.fontSize}" data-k="font" /> px <span class="dim">(${keysIn("⌘= większa, ⌘- mniejsza, ⌘0 zwykła")})</span></label>
      <label class="line">Strony poza desk (↗, ⌥klik) <select data-k="browser">${[
        ["auto", "Brave, a gdy go nie ma Chrome"],
        ["brave", "Brave"],
        ["chrome", "Chrome"],
        ["system", "przeglądarka systemowa"],
      ]
        .map(([v, label]) => `<option value="${v}" ${(layout.browser || "auto") === v ? "selected" : ""}>${label}</option>`)
        .join("")}</select></label>
      <h3>Motyw</h3>
      <div class="themes">${THEMES.map(
        (t) => `<button class="theme ${t.id === (layout.theme || DEFAULT_THEME) ? "sel" : ""}" data-theme="${t.id}" style="background:${t.bg};color:${t.fg}">
          <span class="swatches">${[1, 2, 3, 4, 5].map((i) => `<i style="background:${t.ansi[i]}"></i>`).join("")}</span>${escapeHtml(t.name)}</button>`,
      ).join("")}</div>
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
            a.installed
              ? `<span class="tag open">jest</span>`
              : a.installable
                ? `<span class="tag">zainstaluje się przy otwarciu</span>`
                : `<span class="tag">brak w PATH</span>`
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
      <h3>Kawa <span class="dim">(${escapeHtml(keyText("coffee"))})</span></h3>
      <div class="line dim">Wyłącza się sama przy przegrzaniu albo baterii ≤15%. ${
        this.ctx.coffee().helperInstalled
          ? `Zgoda na tryb z klapką jest zainstalowana (/etc/sudoers.d/desk-pmset, tylko pmset disablesleep). <button data-k="coffee-rm">Usuń zgodę</button>`
          : "Tryb z zamkniętą klapką zapyta raz o hasło administratora."
      }</div>
      <h3>Aktualizacje</h3>
      <div class="line"><span class="dim">desk <span data-k="version"></span> · sprawdza raz dziennie (przez GitHub CLI, gh)</span> <button data-k="update">Sprawdź teraz</button></div>
      </div>
      <div class="pane" data-pane="dropdown" ${this.tab === "dropdown" ? "" : "hidden"}>
      <label class="line"><input type="checkbox" data-k="dropdown" ${layout.dropdown ? "checked" : ""}/> ${escapeHtml(hotkey)} wysuwa desk z góry ekranu</label>
      <p class="dim small">Jak w iTerm: działa w każdej aplikacji, a drugie ${escapeHtml(hotkey)} chowa desk. Skrót zmienisz w zakładce Skróty, a wyłączenie przywraca zwykłe okno.</p>
      <label class="line">Wysokość <input type="number" min="20" max="100" step="5" value="${layout.dropdownHeight}" data-k="dropdown-height" ${off}/> % ekranu <span class="dim">(albo przeciągnij dolną krawędź, desk ją zapamięta)</span></label>
      <label class="line">Animacja <input type="number" min="0" max="1000" step="50" value="${layout.dropdownAnimation}" data-k="dropdown-animation" ${off}/> ms <span class="dim">(0 bez animacji)</span></label>
      <label class="line"><input type="checkbox" data-k="dropdown-autohide" ${layout.dropdownAutohide ? "checked" : ""} ${off}/> Chowaj, gdy klikniesz inną aplikację</label>
      </div>
      <div class="pane" data-pane="keys" ${this.tab === "keys" ? "" : "hidden"}>
      <p class="dim small">Kliknij skrót i naciśnij nowy (Esc anuluje), ↺ przywraca domyślny.</p>
      <div class="shortcut-list"><div class="group">Dropdown</div>${this.keyButton("dropdown", hotkey, off)}<span>wysuwa i chowa desk z góry ekranu, w każdej aplikacji${
        layout.dropdown ? "" : ` <span class="dim">(wyłączony w zakładce Dropdown)</span>`
      }${
        layout.dropdownHotkey && layout.dropdownHotkey !== DEFAULT_HOTKEY()
          ? ` <button class="reset" data-unbind="dropdown" title="domyślny">↺</button>`
          : ""
      }</span>${GROUPS.map(
        (group) =>
          `<div class="group">${escapeHtml(group)}</div>` +
          BINDINGS.filter((b) => b.group === group)
            .map(
              (b) =>
                `${this.keyButton(b.id, keyText(b.id) || "brak")}<span>${escapeHtml(b.label)}${
                  layout.keys[b.id] ? ` <button class="reset" data-unbind="${b.id}" title="domyślny">↺</button>` : ""
                }</span>`,
            )
            .join("") +
          FIXED.filter((f) => f.group === group)
            .map((f) => `<kbd>${escapeHtml(shortcut(f.keys))}</kbd><span>${escapeHtml(f.label)} <span class="dim">(stały)</span></span>`)
            .join(""),
      ).join("")}</div>
      <p class="dim small">Zwykłe Esc trafia do terminala, więc dalej przerywa Claude.</p>
      </div>
      <p class="dim small">Ustawienia: ~/Library/Application Support/desk/layout.json · sesje: tmux -L desk ls</p>`;

    box.querySelectorAll<HTMLButtonElement>("[data-tab]").forEach((b) =>
      b.addEventListener("click", () => {
        this.tab = b.dataset.tab as typeof this.tab;
        this.recording = null;
        this.render();
        box.scrollTop = 0;
      }),
    );

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
    box.querySelectorAll<HTMLButtonElement>("[data-theme]").forEach((b) =>
      b.addEventListener("click", () => {
        this.ctx.setTheme(b.dataset.theme!);
        this.render();
      }),
    );
    void getVersion().then((v) => {
      const el = this.el?.querySelector('[data-k="version"]');
      if (el) el.textContent = v;
    });
    box.querySelector<HTMLButtonElement>('[data-k="update"]')!.addEventListener("click", async (e) => {
      const b = e.target as HTMLButtonElement;
      b.disabled = true;
      b.textContent = "sprawdzam…";
      await this.ctx.checkUpdate();
      b.disabled = false;
      b.textContent = "Sprawdź teraz";
    });
    box.querySelector<HTMLInputElement>('[data-k="font"]')!.addEventListener("change", (e) => {
      const px = Number((e.target as HTMLInputElement).value);
      if (px > 0) this.ctx.setFontSize(px);
    });
    box.querySelector<HTMLInputElement>('[data-k="dropdown"]')!.addEventListener("change", async (e) => {
      await this.ctx.setDropdown({ dropdown: (e.target as HTMLInputElement).checked });
      this.render();
    });
    box.querySelector<HTMLInputElement>('[data-k="dropdown-height"]')!.addEventListener("change", (e) => {
      const pct = Number((e.target as HTMLInputElement).value);
      if (pct >= 20 && pct <= 100) void this.ctx.setDropdown({ dropdownHeight: Math.round(pct) });
    });
    box.querySelectorAll<HTMLButtonElement>("[data-record]").forEach((b) =>
      b.addEventListener("click", () => {
        this.recording = this.recording === b.dataset.record ? null : b.dataset.record!;
        this.render();
      }),
    );
    box.querySelectorAll<HTMLButtonElement>("[data-unbind]").forEach((b) =>
      b.addEventListener("click", async () => {
        this.recording = null;
        const id = b.dataset.unbind!;
        if (id === "dropdown") await this.ctx.setDropdown({ dropdownHotkey: DEFAULT_HOTKEY() });
        else this.ctx.setKey(id, null);
        this.render();
      }),
    );
    box.querySelector<HTMLInputElement>('[data-k="dropdown-animation"]')!.addEventListener("change", (e) => {
      const ms = Number((e.target as HTMLInputElement).value);
      if (ms >= 0 && ms <= 1000) void this.ctx.setDropdown({ dropdownAnimation: Math.round(ms) });
    });
    box.querySelector<HTMLInputElement>('[data-k="dropdown-autohide"]')!.addEventListener("change", (e) => {
      void this.ctx.setDropdown({ dropdownAutohide: (e.target as HTMLInputElement).checked });
    });
    box.querySelector<HTMLSelectElement>('[data-k="browser"]')!.addEventListener("change", (e) => {
      this.ctx.setBrowser((e.target as HTMLSelectElement).value);
    });
    box.querySelector<HTMLInputElement>('[data-k="auto-layout"]')!.addEventListener("change", (e) => {
      this.ctx.setAutoLayout((e.target as HTMLInputElement).checked);
      this.render();
    });
    box.querySelector<HTMLInputElement>('[data-k="notify"]')!.addEventListener("change", (e) => {
      this.ctx.setNotifications((e.target as HTMLInputElement).checked);
    });
    box.querySelector<HTMLInputElement>('[data-k="tmux-menu"]')!.addEventListener("change", (e) => {
      this.ctx.setTmuxMenu((e.target as HTMLInputElement).checked);
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
    box.querySelector<HTMLButtonElement>('[data-k="coffee-rm"]')?.addEventListener("click", async () => {
      await this.ctx.uninstallCoffee();
      this.render();
    });
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
