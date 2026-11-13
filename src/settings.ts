// ⌘O options: lane count, notifications, which projects start with the app,
// hand-added folders.
import { getVersion } from "@tauri-apps/api/app";
import { DEFAULT_THEME, THEMES } from "./themes";
import { api, type AccountInfo, type AgentInfo, type ClaudeAccount, type CoffeeStatus, type Layout, type Project, type PromptAi } from "./api";
import { accountColor, withAccount, withoutAccount } from "./accounts";
import type { LinkClick } from "./links";
import { PLATFORM, shortcut } from "./platform";
import { rows, shortPath } from "./projects";
import { escapeHtml } from "./tile";
import { acceleratorFor, DEFAULT_HOTKEY, hotkeyLabel } from "./hotkey";
import { LANGUAGES, language, t, type Language, type MessageKey } from "./i18n";
import { acceleratorsOf, bindingLabel, BINDINGS, clashWith, FIXED, groupLabel, GROUPS, keysIn, keyText } from "./keys";
import { clampLimit, DEFAULT_LIMIT, MAX_LIMIT, MIN_LIMIT } from "./thermal";
import { toast } from "./ui";

export interface SettingsContext {
  home: () => string;
  layout: () => Layout;
  projects: () => Promise<Project[]>;
  setLaneCount: (n: number) => void;
  setAutoLayout: (on: boolean) => void;
  setTheme: (id: string) => void;
  setBrowser: (id: string) => void;
  /** What a plain click on a terminal link does (links.ts). */
  setLinkClick: (mode: LinkClick) => void;
  setFontSize: (px: number) => void;
  setLanguage: (id: Language) => void;
  setDropdown: (
    change: Partial<Pick<Layout, "dropdown" | "dropdownHeight" | "dropdownAutohide" | "dropdownHotkey" | "dropdownAnimation">>,
  ) => Promise<void>;
  /** A desk shortcut set in Options, or back to its default (null). */
  setKey: (id: string, accelerator: string | null) => void;
  /** Ask GitHub for a newer desk now. */
  checkUpdate: () => Promise<void>;
  setNotifications: (on: boolean) => void;
  setTmuxMenu: (on: boolean) => void;
  setPromptAi: (change: Partial<PromptAi>) => void;
  toggleAutostart: (path: string) => void;
  addFolder: () => Promise<void>;
  removeFolder: (path: string) => void;
  /** °C above which desk slows its sessions; 0: never (thermal.ts). */
  setThermalLimit: (celsius: number) => void;
  coffee: () => CoffeeStatus;
  uninstallCoffee: () => Promise<void>;
  agents: () => AgentInfo[];
  addAgent: (id: string, command: string) => Promise<void>;
  removeAgent: (id: string) => Promise<void>;
  /** Claude accounts changed (accounts.ts): saved at once, Rust reads them. */
  setAccounts: (change: Pick<Layout, "claudeAccounts" | "tileAccounts" | "projectAccounts">) => Promise<void>;
}

/** The groups of Options, in the sidebar's order. */
export type Group = "general" | "look" | "window" | "terminal" | "projects" | "claude" | "power" | "keys";

const GROUP_NAMES: Record<Group, MessageKey> = {
  general: "settings.general",
  look: "settings.look",
  window: "settings.window",
  terminal: "settings.terminal",
  projects: "settings.projects",
  claude: "settings.claude",
  power: "settings.power",
  keys: "settings.keys",
};

/** A folder for an account: in the home folder (~/...) or a full path. */
const ACCOUNT_DIR = /^(~[\\/]|\/|[a-z]:[\\/]|\\\\)/i;

export class Settings {
  /** Pasted WireGuard config for prompt rewriting: saved by Rust to an
   *  owner-only file; the field is emptied after saving, only the server shows. */
  private bindVpn(box: HTMLElement): void {
    const area = box.querySelector<HTMLTextAreaElement>('[data-k="prompt-ai-vpn"]')!;
    const status = box.querySelector<HTMLElement>('[data-k="prompt-ai-vpn-status"]')!;
    const rm = box.querySelector<HTMLButtonElement>('[data-k="prompt-ai-vpn-rm"]')!;
    const saveBtn = box.querySelector<HTMLButtonElement>('[data-k="prompt-ai-vpn-save"]')!;
    const error = box.querySelector<HTMLElement>('[data-k="prompt-ai-vpn-error"]')!;
    // A paste saves at once and clears the field (the private key is not left
    // on screen), so an empty field with a saved config is not an error.
    let saved: string | null = null;
    const show = (server: string | null) => {
      saved = server;
      status.textContent = server ? t("settings.promptAiVpnSaved", { server }) : t("settings.promptAiVpnNone");
      rm.hidden = !server;
    };
    // The error stays under the field until the next try: a toast was missed.
    const save = (conf: string) => {
      error.textContent = "";
      return api
        .setPromptAiVpn(conf)
        .then((server) => {
          area.value = "";
          show(server);
          if (server) toast(t("settings.promptAiVpnSaved", { server }));
        })
        .catch((err) => {
          error.textContent = String(err);
        });
    };
    void api.promptAiVpn().then(show).catch(() => show(null));
    // Saved right after a paste (the value is there on the next tick), on
    // leaving the field, or with the button.
    area.addEventListener("paste", () => setTimeout(() => area.value.trim() && void save(area.value)));
    area.addEventListener("change", () => {
      if (area.value.trim()) void save(area.value);
    });
    saveBtn.addEventListener("click", () => {
      if (area.value.trim()) void save(area.value);
      else if (saved) toast(t("settings.promptAiVpnSaved", { server: saved }));
      else error.textContent = t("settings.promptAiVpnEmpty");
    });
    rm.addEventListener("click", () => void save(""));
  }

  /** The accounts as Options lists them: with none set, the default one alone. */
  private accountRows(layout: Layout): ClaudeAccount[] {
    return layout.claudeAccounts?.length ? layout.claudeAccounts : [{ id: "main", name: t("settings.accountMain"), dir: "" }];
  }

  /** Names, removal, a new account, and who is logged in on each. */
  private bindAccounts(box: HTMLElement): void {
    const layout = this.ctx.layout();
    const rows = this.accountRows(layout);
    const change = (claudeAccounts: ClaudeAccount[]) =>
      this.ctx.setAccounts({ claudeAccounts, tileAccounts: layout.tileAccounts, projectAccounts: layout.projectAccounts }).then(() => this.render());
    box.querySelectorAll<HTMLInputElement>("[data-acct-name]").forEach((input) =>
      input.addEventListener("change", () => {
        const name = input.value.trim();
        const id = input.dataset.acctName!;
        if (!name || rows.some((a) => a.id !== id && a.name.toLowerCase() === name.toLowerCase())) {
          if (name) toast(t("settings.accountExists"));
          input.value = rows.find((a) => a.id === id)?.name ?? "";
          return;
        }
        void change(rows.map((a) => (a.id === id ? { ...a, name } : a)));
      }),
    );
    box.querySelectorAll<HTMLButtonElement>("[data-rm-acct]").forEach((b) =>
      b.addEventListener("click", () => {
        void this.ctx.setAccounts(withoutAccount(layout, b.dataset.rmAcct!)).then(() => this.render());
      }),
    );
    box.querySelector<HTMLButtonElement>('[data-k="acct-add"]')!.addEventListener("click", () => {
      const name = box.querySelector<HTMLInputElement>('[data-k="acct-name"]')!.value.trim();
      const dir = box.querySelector<HTMLInputElement>('[data-k="acct-dir"]')!.value.trim();
      if (!name) return;
      if (rows.some((a) => a.name.toLowerCase() === name.toLowerCase())) return toast(t("settings.accountExists"));
      if (dir && !ACCOUNT_DIR.test(dir)) return toast(t("settings.accountDirBad"));
      void change(withAccount(layout.claudeAccounts ?? [], name, dir, t("settings.accountMain")));
    });
    // Who is logged in, read from each account's own Claude config.
    void api
      .claudeAccounts()
      .then((infos: AccountInfo[]) => {
        for (const a of rows) {
          const el = this.el?.querySelector<HTMLElement>(`[data-acct-email="${CSS.escape(a.id)}"]`);
          const info = infos.find((i) => i.id === a.id) ?? (rows.length === 1 ? infos[0] : undefined);
          if (el) el.textContent = info?.email ? `· ${info.email}` : `· ${t("settings.accountNoLogin")}`;
        }
      })
      .catch(() => {});
  }

  private el: HTMLElement | null = null;
  private projects: Project[] = [];
  private filter = "";
  /** Waiting for a new shortcut: "dropdown" or a binding id (keys.ts). */
  private recording: string | null = null;
  private tab: Group = "general";

  constructor(private ctx: SettingsContext) {}

  get isOpen(): boolean {
    return this.el !== null;
  }

  /** Keys go to the shortcut field, not to desk's shortcuts. */
  get isRecording(): boolean {
    return this.recording !== null;
  }

  /** Open on `group`, or on the group seen last; close when open. */
  async toggle(group?: Group): Promise<void> {
    if (this.el) return this.close();
    if (group) this.tab = group;
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
      toast(t("settings.taken", { key: hotkeyLabel(accelerator), what: clash }));
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
      on ? t("settings.press") : escapeHtml(label)
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
    const linkClick: LinkClick = layout.linkClick === "browser" || layout.linkClick === "none" ? layout.linkClick : "desk";
    const keyNames = PLATFORM === "mac" ? { mod: "⌘", alt: "⌥" } : { mod: "Ctrl", alt: "Alt" };
    const linkHint = t(
      linkClick === "browser" ? "settings.linkClickHintBrowser" : linkClick === "none" ? "settings.linkClickHintNone" : "settings.linkClickHintDesk",
      keyNames,
    );
    // A section of a group. `row`: the whole section is one search hit
    // ("header": found by its header alone); otherwise its lines are.
    const section = (head: string, body: string, row: "" | "row" | "header" = ""): string =>
      `<section data-section${row === "header" ? ' data-row="header"' : row ? " data-row" : ""}>${head ? `<h3>${head}</h3>` : ""}${body}</section>`;
    const panes: [Group, string][] = [
      [
        "general",
        section(
          "",
          `<label class="line" data-row>${t("settings.language")} <select data-k="language">${LANGUAGES.map(
            (l) => `<option value="${l.id}" ${language() === l.id ? "selected" : ""}>${l.name}</option>`,
          ).join("")}</select></label>
      <label class="line" data-row><input type="checkbox" data-k="notify" ${layout.notifications ? "checked" : ""}/> ${t("settings.notify")}</label>`,
        ) +
          section(
            t("settings.updates"),
            `<div class="line" data-row><span class="dim">desk <span data-k="version"></span> · ${t("settings.updatesHow")}</span> <button data-k="update">${t("settings.checkNow")}</button></div>`,
          ),
      ],
      [
        "look",
        section(
          t("settings.theme"),
          `<div class="themes">${THEMES.map(
            (t) => `<button class="theme ${t.id === (layout.theme || DEFAULT_THEME) ? "sel" : ""}" data-theme="${t.id}" style="background:${t.bg};color:${t.fg}">
          <span class="swatches">${[1, 2, 3, 4, 5].map((i) => `<i style="background:${t.ansi[i]}"></i>`).join("")}</span>${escapeHtml(t.name)}</button>`,
          ).join("")}</div>`,
          "row",
        ) +
          section(
            t("settings.terminalSection"),
            `<label class="line" data-row>${t("settings.font")} <input type="number" min="8" max="24" step="0.5" value="${layout.fontSize}" data-k="font" /> px <span class="dim">(${keysIn(t("settings.fontHint"))})</span></label>`,
          ),
      ],
      [
        "window",
        section(
          "",
          `<label class="line" data-row><input type="checkbox" data-k="auto-layout" ${layout.autoLayout ? "checked" : ""}/> ${t("settings.autoLayout")} <span class="dim">${keysIn(t("settings.autoLayoutHint"))}</span></label>
      <label class="line" data-row>${t("settings.lanes")} <input type="number" min="1" max="8" value="${layout.lanes.length}" data-k="lanes" ${layout.autoLayout ? "disabled" : ""}/></label>`,
        ) +
          section(
            t("settings.dropdown"),
            `<label class="line" data-row><input type="checkbox" data-k="dropdown" ${layout.dropdown ? "checked" : ""}/> ${t("settings.dropdownOn", { hotkey: escapeHtml(hotkey) })}</label>
      <p class="dim small" data-row>${t("settings.dropdownAbout", { hotkey: escapeHtml(hotkey) })}</p>
      <label class="line" data-row>${t("settings.height")} <input type="number" min="20" max="100" step="5" value="${layout.dropdownHeight}" data-k="dropdown-height" ${off}/> ${t("settings.heightUnit")} <span class="dim">${t("settings.heightHint")}</span></label>
      <label class="line" data-row>${t("settings.animation")} <input type="number" min="0" max="1000" step="50" value="${layout.dropdownAnimation}" data-k="dropdown-animation" ${off}/> ms <span class="dim">${t("settings.animationHint")}</span></label>
      <label class="line" data-row><input type="checkbox" data-k="dropdown-autohide" ${layout.dropdownAutohide ? "checked" : ""} ${off}/> ${t("settings.autohide")}</label>`,
          ),
      ],
      [
        "terminal",
        section(
          t("settings.links"),
          `<div class="line" data-row><span>${t("settings.linkClick")}</span><span class="seg">${(
            [
              ["desk", t("settings.linkClickDesk")],
              ["browser", t("settings.linkClickBrowser")],
              ["none", t("settings.linkClickNone")],
            ] as const
          )
            .map(([v, label]) => `<button class="${linkClick === v ? "sel" : ""}" data-link-click="${v}">${label}</button>`)
            .join("")}</span><span class="dim">${linkHint}</span></div>
      <label class="line" data-row>${t("settings.browser")} <select data-k="browser">${[
        ["auto", t("settings.browserAuto")],
        ["brave", "Brave"],
        ["chrome", "Chrome"],
        ["system", t("settings.browserSystem")],
      ]
        .map(([v, label]) => `<option value="${v}" ${(layout.browser || "auto") === v ? "selected" : ""}>${label}</option>`)
        .join("")}</select></label>`,
        ) +
          section(
            t("settings.terminalSection"),
            `<label class="line" data-row><input type="checkbox" data-k="tmux-menu" ${layout.tmuxMenu ? "checked" : ""}/> ${t("settings.tmuxMenu")} <span class="dim">${t("settings.tmuxMenuHint")}</span></label>`,
          ),
      ],
      [
        "projects",
        section(
          `${t("settings.autostart")} <span class="dim">(${auto.size})</span>`,
          `<input type="text" class="filter" placeholder="${t("settings.filter")}" value="${escapeHtml(this.filter)}" data-k="filter" spellcheck="false" />
      <div class="checks">${
        list.length
          ? list
              .map(
                (r) => `<label class="${r.parent ? "child" : ""}"><input type="checkbox" data-auto="${escapeHtml(r.project.path)}" ${
                  auto.has(r.project.path) ? "checked" : ""
                }/> <b>${escapeHtml(r.project.name)}</b> <span class="dim">${escapeHtml(shortPath(r.project.path, this.ctx.home()))}</span></label>`,
              )
              .join("")
          : `<div class="dim">${t("common.searchingProjects")}</div>`
      }</div>`,
          "header",
        ) +
          section(
            t("settings.folders"),
            `<div class="folders">${layout.extraFolders
              .map(
                (f) => `<div class="folder"><span>${escapeHtml(shortPath(f, this.ctx.home()))}</span><button data-rm="${escapeHtml(f)}">${t("common.remove")}</button></div>`,
              )
              .join("")}<button data-k="add">${t("settings.addFolder")}</button></div>`,
            "row",
          ),
      ],
      [
        "claude",
        section(
          `${t("settings.accounts")} <span class="dim">${t("settings.accountsHint")}</span>`,
          `<p class="dim small">${keysIn(t("settings.accountsAbout"))}</p>
      <div class="accounts">${this.accountRows(layout)
        .map(
          (a, i) => `<div class="account-row"><i class="acct" style="background:${accountColor(layout, a.id)}"></i><input type="text" data-acct-name="${escapeHtml(
            a.id,
          )}" value="${escapeHtml(a.name)}" spellcheck="false" /> <span class="dim">${escapeHtml(a.dir || t("settings.accountDefaultDir"))}</span> <span class="dim" data-acct-email="${escapeHtml(
            a.id,
          )}"></span>${i > 0 ? ` <button data-rm-acct="${escapeHtml(a.id)}">${t("common.remove")}</button>` : ""}</div>`,
        )
        .join("")}
        <div class="line"><input type="text" data-k="acct-name" placeholder="${t("settings.accountName")}" spellcheck="false" /><input type="text" data-k="acct-dir" placeholder="${t(
          "settings.accountDir",
        )}" spellcheck="false" /><button data-k="acct-add">${t("settings.accountAdd")}</button></div>
      </div>`,
          "row",
        ) +
          section(
            `${t("settings.agents")} <span class="dim">${t("settings.agentsHint")}</span>`,
            `<div class="agents">${this.ctx
              .agents()
              .map(
                (a) => `<div class="agent-row"><b>${escapeHtml(a.id)}</b> <span class="dim">${escapeHtml(a.command || "login shell")}</span> ${
                  a.installed
                    ? `<span class="tag open">${t("settings.agentInstalled")}</span>`
                    : a.installable
                      ? `<span class="tag">${t("common.installsOnOpen")}</span>`
                      : `<span class="tag">${t("settings.agentMissing")}</span>`
                }${a.builtin ? "" : ` <button data-rm-agent="${escapeHtml(a.id)}">${t("common.remove")}</button>`}</div>`,
              )
              .join("")}
        <div class="line"><input type="text" data-k="agent-id" placeholder="${t("settings.agentId")}" spellcheck="false" /><input type="text" data-k="agent-cmd" placeholder="${t("settings.agentCommand")}" spellcheck="false" /><button data-k="agent-add">${t("settings.addAgent")}</button></div>
      </div>`,
            "row",
          ) +
          section(
            `${t("settings.promptAi")} <span class="dim">${t("settings.promptAiHint")}</span>`,
            `<div class="prompt-ai">
      <label class="line"><input type="checkbox" data-k="prompt-ai-on" ${layout.promptAi?.enabled ? "checked" : ""}/> ${t("settings.promptAiOn")}</label>
      <label class="line">${t("settings.promptAiUrl")} <input type="text" data-k="prompt-ai-url" value="${escapeHtml(layout.promptAi?.url ?? "")}" placeholder="http://host:port/v1" spellcheck="false" /></label>
      <label class="line">${t("settings.promptAiModel")} <input type="text" data-k="prompt-ai-model" value="${escapeHtml(layout.promptAi?.model ?? "")}" spellcheck="false" /></label>
      <label class="line col">${t("settings.promptAiVpn")} <span class="dim" data-k="prompt-ai-vpn-status"></span>
        <textarea data-k="prompt-ai-vpn" rows="3" spellcheck="false" placeholder="${t("settings.promptAiVpnHint")}"></textarea></label>
      <div class="line"><button data-k="prompt-ai-vpn-save">${t("settings.promptAiVpnSave")}</button><button data-k="prompt-ai-vpn-rm" hidden>${t("settings.promptAiVpnRemove")}</button><span class="vpn-error" data-k="prompt-ai-vpn-error"></span></div>
      </div>`,
            "row",
          ),
      ],
      [
        "power",
        section(
          `${t("settings.coffee")} <span class="dim">(${escapeHtml(keyText("coffee"))})</span>`,
          `<div class="line dim" data-row>${t("settings.coffeeGuard")} ${
            this.ctx.coffee().helperInstalled
              ? `${t("settings.coffeeHelper")} <button data-k="coffee-rm">${t("settings.coffeeRemove")}</button>`
              : t("settings.coffeeAsk")
          }</div>`,
        ) +
          section(
            t("settings.thermal"),
            `<label class="line" data-row><input type="checkbox" data-k="thermal-on" ${layout.thermalLimit ? "checked" : ""}/> ${t("settings.thermalOn")}</label>
      <label class="line" data-row>${t("settings.thermalLimit")} <input type="number" min="${MIN_LIMIT}" max="${MAX_LIMIT}" step="1" value="${layout.thermalLimit || DEFAULT_LIMIT}" data-k="thermal-limit" ${layout.thermalLimit ? "" : "disabled"}/> °C</label>
      <p class="dim small" data-row>${t("settings.thermalAbout")}</p>`,
          ),
      ],
      [
        "keys",
        section("", `<p class="dim small" data-row>${t("settings.keysAbout")}</p>`) +
          `<div class="shortcut-list">${section(
            "Dropdown",
            `<div class="shortcut" data-row>${this.keyButton("dropdown", hotkey, off)}<span>${t("settings.dropdownKey")}${
              layout.dropdown ? "" : ` <span class="dim">${t("settings.dropdownKeyOff")}</span>`
            }${
              layout.dropdownHotkey && layout.dropdownHotkey !== DEFAULT_HOTKEY()
                ? ` <button class="reset" data-unbind="dropdown" title="${t("settings.default")}">↺</button>`
                : ""
            }</span></div>`,
          )}${GROUPS.map((group) =>
            section(
              escapeHtml(groupLabel(group)),
              BINDINGS.filter((b) => b.group === group)
                .map(
                  (b) =>
                    `<div class="shortcut" data-row>${this.keyButton(b.id, keyText(b.id) || t("settings.noKey"))}<span>${escapeHtml(bindingLabel(b.id))}${
                      layout.keys[b.id] ? ` <button class="reset" data-unbind="${b.id}" title="${t("settings.default")}">↺</button>` : ""
                    }</span></div>`,
                )
                .join("") +
                FIXED.filter((f) => f.group === group)
                  .map(
                    (f) =>
                      `<div class="shortcut" data-row><kbd>${escapeHtml(shortcut(f.keys))}</kbd><span>${escapeHtml(t(f.label))} <span class="dim">${t("settings.fixed")}</span></span></div>`,
                  )
                  .join(""),
            ),
          ).join("")}</div>` +
          section("", `<p class="dim small" data-row>${t("settings.escNote")}</p>`),
      ],
    ];
    // Every group is in the page (the hidden ones too), so every field keeps its listener.
    box.innerHTML = `
      <div class="settings-head"><h2>${t("settings.title")}</h2></div>
      <div class="settings-body">
      <nav class="settings-nav">${panes
        .map(
          ([id]) =>
            `<button class="tab ${this.tab === id ? "sel" : ""}" data-tab="${id}"><span>${t(GROUP_NAMES[id])}</span><span class="count"></span></button>`,
        )
        .join("")}</nav>
      <div class="settings-panes">${panes
        .map(
          ([id, html]) =>
            `<div class="pane" data-pane="${id}" ${this.tab === id ? "" : "hidden"}><h2 class="pane-title">${t(GROUP_NAMES[id])}</h2>${html}</div>`,
        )
        .join("")}</div>
      </div>
      <p class="dim small settings-foot">${t("settings.where", { path: "~/Library/Application Support/desk/layout.json", sessions: "tmux -L desk ls" })}</p>`;

    box.querySelectorAll<HTMLButtonElement>("[data-tab]").forEach((b) =>
      b.addEventListener("click", () => {
        this.tab = b.dataset.tab as Group;
        this.recording = null;
        this.render();
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
      b.textContent = t("settings.checking");
      await this.ctx.checkUpdate();
      b.disabled = false;
      b.textContent = t("settings.checkNow");
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
    box.querySelector<HTMLSelectElement>('[data-k="language"]')!.addEventListener("change", (e) => {
      this.ctx.setLanguage((e.target as HTMLSelectElement).value as Language);
      this.render();
    });
    box.querySelectorAll<HTMLButtonElement>("[data-link-click]").forEach((b) =>
      b.addEventListener("click", () => {
        this.ctx.setLinkClick(b.dataset.linkClick as LinkClick);
        this.render();
      }),
    );
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
    const thermalLimit = box.querySelector<HTMLInputElement>('[data-k="thermal-limit"]')!;
    box.querySelector<HTMLInputElement>('[data-k="thermal-on"]')!.addEventListener("change", (e) => {
      const on = (e.target as HTMLInputElement).checked;
      this.ctx.setThermalLimit(on ? clampLimit(Number(thermalLimit.value)) : 0);
      this.render();
    });
    thermalLimit.addEventListener("change", () => {
      const celsius = clampLimit(Number(thermalLimit.value));
      thermalLimit.value = String(celsius);
      this.ctx.setThermalLimit(celsius);
    });
    box.querySelector<HTMLInputElement>('[data-k="tmux-menu"]')!.addEventListener("change", (e) => {
      this.ctx.setTmuxMenu((e.target as HTMLInputElement).checked);
    });
    box.querySelector<HTMLInputElement>('[data-k="prompt-ai-on"]')!.addEventListener("change", (e) => {
      this.ctx.setPromptAi({ enabled: (e.target as HTMLInputElement).checked });
    });
    for (const [k, field] of [
      ["prompt-ai-url", "url"],
      ["prompt-ai-model", "model"],
    ] as const) {
      box.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[data-k="${k}"]`)!.addEventListener("change", (e) => {
        this.ctx.setPromptAi({ [field]: (e.target as HTMLInputElement).value.trim() });
      });
    }
    this.bindVpn(box);
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
    this.bindAccounts(box);
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
