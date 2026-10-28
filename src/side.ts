// Side panel: links, local files, Markdown and diagrams next to the terminals.
// The page itself is a second native webview (no access to desk's commands)
// that Rust keeps over `.side-view`; this file draws the bar above it, tells
// Rust where that box is, and edits text files in desk's own window.

import { listen } from "@tauri-apps/api/event";
import { api, type PreviewBounds } from "./api";
import { t } from "./i18n";
import { keysIn } from "./keys";
import { PLATFORM, shortcut } from "./platform";
import { toast } from "./ui";
import { mention, normalize, rendered } from "./address";
import { FileTree, itemsLabel } from "./treeview";
import { escapeHtml } from "./tile";
import { createEditor, type Editor } from "./editor";

const WIDTH_KEY = "desk.sideWidth";
const TREE_KEY = "desk.sideTree";

/** What the panel needs from the rest of desk. */
export interface SideContext {
  /** Folder of the active session's project, if any. */
  project(): string | null;
  /** Type text into the active session (no Enter). */
  insert(text: string): void;
  /** A plain terminal in this folder. */
  terminal(path: string): void;
  /** Where web pages open outside desk (Layout.browser). */
  browser(): string;
}


const REVEAL = ({ mac: "side.revealMac", windows: "side.revealWindows", linux: "side.revealLinux" } as const)[PLATFORM];

/** Open a link or a local file in the panel (from a terminal link, ⌘F...). */
export function preview(link: string): Promise<void> {
  return Side.current ? Side.current.open(link) : Promise.reject(t("side.notReady"));
}

/** A project file in the panel (⌘F, ⌥↵). */
export function previewFile(path: string, file: string): Promise<void> {
  return Side.current ? Side.current.open(file, path) : Promise.reject(t("side.notReady"));
}

/** A link outside desk: a web page in Brave / Chrome (Options), a file in its default app. */
export function openOutside(link: string): Promise<void> {
  return api.openExternal(link, Side.current?.browserPref());
}

/** A local file (not a web page) is what the address bar shows. */
function isLocal(address: string): boolean {
  return address.startsWith("/") || /^[a-z]:[\\/]/i.test(address);
}

export class Side {
  static current: Side | null = null;
  readonly el: HTMLElement;
  private view: HTMLElement;
  private address: HTMLInputElement;
  private editBtn: HTMLButtonElement;
  private saveBtn: HTMLButtonElement;
  /** A webview exists in Rust (it may be hidden). */
  private created = false;
  private covered = false;
  /** Editing this file in desk's window instead of showing the page. */
  private editing: { path: string; text: string; editor: Editor } | null = null;
  /** A folder card is shown instead of the page (absolute path). */
  private folder: string | null = null;
  private tree: FileTree;
  /** "Brave" / "Chrome", or null when web pages go to the system browser. */
  private browserName: string | null = null;
  /** The page has a password field: offer the real browser and its password manager. */
  private login = false;

  constructor(
    private onLayout: () => void,
    private ctx: SideContext,
  ) {
    Side.current = this;
    this.el = document.createElement("aside");
    this.el.className = "side";
    this.el.hidden = true;
    this.el.innerHTML = `
      <div class="splitter"></div>
      <div class="side-head">
        <button data-a="back">‹</button>
        <button data-a="forward">›</button>
        <button data-a="reload">⟳</button>
        <input class="side-address" spellcheck="false" />
        <button data-a="login" class="side-login" hidden></button>
        <button data-a="save" class="side-save" hidden></button>
        <button data-a="attach">@</button>
        <button data-a="edit">✎</button>
        <button data-a="external">↗</button>
        <button data-a="files">☰</button>
        <button data-a="close">×</button>
      </div>
      <div class="side-body"><div class="side-view"><div class="side-empty dim" data-hint></div></div></div>`;
    this.view = this.el.querySelector(".side-view")!;
    this.address = this.el.querySelector(".side-address")!;
    this.editBtn = this.el.querySelector('[data-a="edit"]')!;
    this.saveBtn = this.el.querySelector('[data-a="save"]')!;
    this.tree = new FileTree({
      file: (rel) => void this.openInTree(rel),
      folder: (rel) => void this.showFolder(rel),
      edit: (rel) => this.tree.project && void api.openFile(this.tree.project, rel).catch(this.fail),
    });
    this.el.querySelector(".side-body")!.appendChild(this.tree.el);
    this.relabel();
    let treeShown = true;
    try {
      treeShown = localStorage.getItem(TREE_KEY) !== "0";
    } catch {
      // default: shown
    }
    this.tree.el.hidden = !treeShown;
    this.view.addEventListener("click", (e) => {
      const f = (e.target as HTMLElement).closest<HTMLElement>("[data-f]")?.dataset.f;
      if (!f || !this.folder) return;
      if (f === "attach") this.attach(this.folder);
      else if (f === "terminal") this.ctx.terminal(this.folder);
      else if (f === "reveal") void api.revealPath(this.folder).catch(this.fail);
    });
    try {
      const w = Number(localStorage.getItem(WIDTH_KEY));
      if (w > 0) this.el.style.width = `${w}px`;
    } catch {
      // storage unavailable: default width
    }

    this.el.querySelector(".side-head")!.addEventListener("click", (e) => {
      const a = (e.target as HTMLElement).closest<HTMLElement>("[data-a]")?.dataset.a;
      if (a === "close") this.close();
      else if (a === "files") this.toggleTree();
      else if (a === "attach") this.attach(this.folder ?? this.editing?.path ?? this.address.value);
      else if (a === "edit") void (this.editing ? this.stopEditing() : this.edit());
      else if (a === "save") void this.save();
      else if (a === "external" && this.address.value) void openOutside(this.editing?.path ?? this.address.value).catch(this.fail);
      else if (a === "login" && this.address.value) void openOutside(this.address.value).catch(this.fail);
      else if ((a === "back" || a === "forward" || a === "reload") && this.created && !this.editing) {
        void api.previewNav(a).catch(this.fail);
      }
    });
    this.address.addEventListener("keydown", (e) => {
      if (e.key === "Enter") void this.open(normalize(this.address.value));
      if (e.key === "Escape") this.address.blur();
    });
    this.splitter();
    new ResizeObserver(() => this.place()).observe(this.view);
    window.addEventListener("resize", () => this.place());
    // A native view sits above the page: hide it while a dialog or the lock covers it.
    new MutationObserver(() => this.cover()).observe(document.body, { childList: true });
    this.covered = document.querySelector(".overlay, .lock-screen") !== null;
    void listen<boolean>("preview-login", (e) => {
      this.login = e.payload;
      this.syncButtons();
    });
    void listen<string>("preview-url", (e) => {
      if (document.activeElement !== this.address && !this.editing) this.address.value = e.payload;
      this.syncButtons();
    });
  }

  /** Labels in the current language: at start and when Options change it. */
  relabel(): void {
    const titles: [string, string][] = [
      [".splitter", t("side.resize")],
      ['[data-a="back"]', t("side.back")],
      ['[data-a="forward"]', t("side.forward")],
      ['[data-a="reload"]', t("side.reload")],
      ['[data-a="save"]', t("side.saveTitle", { keys: shortcut("⌘S") })],
      ['[data-a="attach"]', t("side.attach")],
      ['[data-a="edit"]', t("side.edit")],
      ['[data-a="external"]', t("side.external")],
      ['[data-a="files"]', t("side.files", { keys: keysIn("⌘F") })],
      ['[data-a="close"]', t("side.close")],
    ];
    for (const [sel, title] of titles) this.el.querySelector<HTMLElement>(sel)!.title = title;
    this.saveBtn.textContent = t("side.save");
    this.address.placeholder = t("side.address");
    const hint = this.view.querySelector<HTMLElement>("[data-hint]");
    if (hint) hint.textContent = t("side.empty");
    this.tree.relabel();
    if (this.isOpen) this.syncButtons();
  }

  private attach(path: string): void {
    const text = mention(this.tree.project, path);
    if (text) this.ctx.insert(text);
    else toast(t("side.unsafePath"));
  }

  get isOpen(): boolean {
    return !this.el.hidden;
  }

  /** `link`, or with `project` a file relative to that project. */
  async open(link: string, project?: string): Promise<void> {
    if (!link.trim()) return;
    if (this.editing && !(await this.stopEditing())) return;
    this.folder = null;
    this.show();
    try {
      this.address.value = project
        ? await api.previewFile(project, link, this.bounds())
        : await api.openPreview(link, this.bounds());
      this.created = true;
      this.view.replaceChildren();
      // A new page shows at once: hide it again if a dialog is open.
      this.sync();
      // Code and plain text open for editing; rendered documents stay a preview (✎ edits them).
      if (isLocal(this.address.value) && !rendered(this.address.value)) await this.edit(true);
    } catch (err) {
      this.fail(err);
      throw err;
    } finally {
      this.syncButtons();
    }
  }

  browserPref(): string {
    return this.ctx.browser();
  }

  /** The browser setting changed (or desk started): name it on the ↗ button. */
  async browserChanged(): Promise<void> {
    this.browserName = await api.browserName(this.ctx.browser()).catch(() => null);
    this.syncButtons();
  }

  /** The browser button: the panel at its address bar, ready for a URL. */
  browse(): void {
    this.show();
    this.sync();
    this.address.focus();
    this.address.select();
  }

  /** ⌘W: show or hide; an empty panel starts at its address bar. */
  toggle(): void {
    if (this.isOpen) return this.close();
    this.show();
    this.sync();
    if (!this.created) this.address.focus();
  }

  close(): void {
    if (!this.isOpen) return;
    this.el.hidden = true;
    this.sync();
    this.onLayout();
  }

  private show(): void {
    if (this.isOpen) return;
    this.el.hidden = false;
    if (!this.tree.el.hidden) void this.tree.load(this.ctx.project());
    this.onLayout();
  }

  /** ⌘F: the panel with the active project's files, filter focused. */
  showFiles(): void {
    this.show();
    if (this.tree.el.hidden) this.toggleTree();
    void this.tree.load(this.ctx.project()).then(() => {
      if (!this.created && !this.folder) void this.showFolder("");
    });
    this.tree.focusFilter();
  }

  /** The active session moved to another project: its files in the tree. */
  projectChanged(): void {
    if (this.isOpen && !this.tree.el.hidden) void this.tree.load(this.ctx.project());
  }

  private toggleTree(): void {
    this.tree.el.hidden = !this.tree.el.hidden;
    try {
      localStorage.setItem(TREE_KEY, this.tree.el.hidden ? "0" : "1");
    } catch {
      // not remembered
    }
    if (!this.tree.el.hidden) void this.tree.load(this.ctx.project());
  }

  private async openInTree(rel: string): Promise<void> {
    const project = this.tree.project;
    if (!project) return;
    await this.open(rel, project).catch(() => undefined);
  }

  /** A folder's card: what is in it and what to do with it. */
  private async showFolder(rel: string): Promise<void> {
    const project = this.tree.project;
    if (!project) return;
    if (this.editing && !(await this.stopEditing())) return;
    const sep = project.includes("\\") ? "\\" : "/";
    const path = rel ? project + sep + rel.split("/").join(sep) : project;
    this.folder = path;
    const name = rel ? rel.slice(rel.lastIndexOf("/") + 1) : (project.split(/[\\/]/).filter(Boolean).pop() ?? project);
    this.view.innerHTML = `<div class="folder-card">
      <svg class="folder-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h4.6l2 2h8.4A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5z"/></svg>
      <div class="folder-name">${escapeHtml(name)}</div>
      <div class="dim">${itemsLabel(this.tree.count(rel))}</div>
      <div class="folder-actions">
        <button data-f="attach">${t("side.addToSession")}</button>
        <button data-f="terminal">${t("side.terminalHere")}</button>
        <button data-f="reveal">${t(REVEAL)}</button>
      </div></div>`;
    this.address.value = path;
    this.sync();
    this.syncButtons();
  }

  /** The page is visible when the panel is open, nothing covers it and no file is being edited. */
  private sync(): void {
    if (this.created) void api.previewVisible(this.isOpen && !this.covered && !this.editing && !this.folder);
  }

  private syncButtons(): void {
    const local = isLocal(this.editing?.path ?? this.address.value);
    this.editBtn.hidden = !!this.folder || !local;
    this.el.querySelector<HTMLElement>('[data-a="attach"]')!.hidden = !this.folder && !local;
    this.editBtn.textContent = this.editing ? t("side.preview") : "✎";
    this.editBtn.title = this.editing ? t("side.saveAndPreview") : t("side.edit");
    this.saveBtn.hidden = !this.editing;
    this.saveBtn.classList.toggle("dirty", this.dirty());
    const ext = this.el.querySelector<HTMLElement>('[data-a="external"]')!;
    const web = !local && !this.folder && /^https?:/i.test(this.address.value);
    ext.textContent = web && this.browserName ? `↗ ${this.browserName}` : "↗";
    ext.title = web
      ? this.browserName
        ? t("side.openIn", { browser: this.browserName })
        : t("side.openInSystem")
      : t("side.openDefault");
    const login = this.el.querySelector<HTMLElement>('[data-a="login"]')!;
    login.hidden = !this.login || !web || !!this.editing;
    login.textContent = this.browserName ? t("side.loginIn", { browser: this.browserName }) : t("side.login");
    login.title = t("side.loginTitle");
  }

  private dirty(): boolean {
    return this.editing !== null && this.editing.editor.value() !== this.editing.text;
  }

  /**
   * Edit the shown file in desk's own window, with syntax colors. `quiet`: it
   * was opened for editing on its own, so a file that is not text just stays
   * in the preview.
   */
  private async edit(quiet = false): Promise<void> {
    const path = this.address.value;
    let text: string;
    try {
      text = await api.readText(path);
    } catch (err) {
      if (!quiet) toast(t("side.editError", { err: String(err) }));
      return;
    }
    if (this.address.value !== path || this.editing) return; // another file meanwhile
    const box = document.createElement("div");
    box.className = "side-editor";
    this.view.replaceChildren(box);
    const editor = createEditor(box, text, path.split(/[\\/]/).pop() ?? path, {
      change: () => this.syncButtons(),
      save: () => void this.save(),
    });
    this.editing = { path, text, editor };
    this.sync();
    this.syncButtons();
    if (!quiet) editor.focus();
  }

  private async save(): Promise<boolean> {
    const ed = this.editing;
    if (!ed) return true;
    const text = ed.editor.value();
    if (text === ed.text) return true; // unchanged: leave the file (and its date) alone
    try {
      await api.writeText(ed.path, text);
      ed.text = text;
      this.syncButtons();
      return true;
    } catch (err) {
      toast(t("side.saveError", { err: String(err) }));
      return false;
    }
  }

  /** Back to the page: save first, then show it again, reloaded. */
  private async stopEditing(): Promise<boolean> {
    if (!(await this.save())) return false;
    this.editing?.editor.destroy();
    this.editing = null;
    this.view.replaceChildren();
    this.sync();
    this.syncButtons();
    if (this.created) void api.previewNav("reload").catch(this.fail);
    return true;
  }

  private bounds(): PreviewBounds {
    const r = this.view.getBoundingClientRect();
    return { x: r.left, y: r.top, width: r.width, height: r.height };
  }

  private place(): void {
    if (this.created && this.isOpen) void api.previewBounds(this.bounds());
  }

  private cover(): void {
    const covered = document.querySelector(".overlay, .lock-screen") !== null;
    if (covered === this.covered) return;
    this.covered = covered;
    this.sync();
  }

  private fail = (err: unknown): void => {
    if (this.created) return toast(t("side.previewError", { err: String(err) }));
    const msg = document.createElement("div");
    msg.className = "side-empty dim";
    msg.textContent = String(err);
    this.view.replaceChildren(msg);
  };

  private splitter(): void {
    const bar = this.el.querySelector<HTMLElement>(".splitter")!;
    bar.addEventListener("pointerdown", (e) => {
      e.preventDefault();
      bar.setPointerCapture(e.pointerId);
      const startX = e.clientX;
      const startW = this.el.getBoundingClientRect().width;
      const move = (ev: PointerEvent) => {
        const max = window.innerWidth - 320;
        const w = Math.min(max, Math.max(280, startW + startX - ev.clientX));
        this.el.style.width = `${w}px`;
      };
      const up = () => {
        bar.removeEventListener("pointermove", move);
        bar.removeEventListener("pointerup", up);
        try {
          localStorage.setItem(WIDTH_KEY, String(Math.round(this.el.getBoundingClientRect().width)));
        } catch {
          // not remembered
        }
        this.onLayout();
      };
      bar.addEventListener("pointermove", move);
      bar.addEventListener("pointerup", up);
    });
  }
}
