// Side panel: links, local files, Markdown and diagrams next to the terminals.
// The page itself is a second native webview (no access to desk's commands)
// that Rust keeps over `.side-view`; this file draws the bar above it, tells
// Rust where that box is, and edits text files in desk's own window.

import { listen } from "@tauri-apps/api/event";
import { api, type PreviewBounds } from "./api";
import { PLATFORM, shortcut } from "./platform";
import { toast } from "./ui";
import { mention, normalize } from "./address";
import { FileTree, itemsLabel } from "./treeview";
import { escapeHtml } from "./tile";

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
}


const REVEAL = { mac: "Pokaż w Finderze", windows: "Pokaż w Eksploratorze", linux: "Pokaż w menedżerze plików" }[PLATFORM];

/** Open a link or a local file in the panel (from a terminal link, ⌘F...). */
export function preview(link: string): Promise<void> {
  return Side.current ? Side.current.open(link) : Promise.reject("panel podglądu nie jest gotowy");
}

/** A project file in the panel (⌘F, ⌥↵). */
export function previewFile(path: string, file: string): Promise<void> {
  return Side.current ? Side.current.open(file, path) : Promise.reject("panel podglądu nie jest gotowy");
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
  private editing: { path: string; text: string; area: HTMLTextAreaElement } | null = null;
  /** A folder card is shown instead of the page (absolute path). */
  private folder: string | null = null;
  private tree: FileTree;

  constructor(
    private onLayout: () => void,
    private ctx: SideContext,
  ) {
    Side.current = this;
    this.el = document.createElement("aside");
    this.el.className = "side";
    this.el.hidden = true;
    this.el.innerHTML = `
      <div class="splitter" title="Przeciągnij, aby zmienić szerokość"></div>
      <div class="side-head">
        <button data-a="back" title="Wstecz">‹</button>
        <button data-a="forward" title="Dalej">›</button>
        <button data-a="reload" title="Odśwież">⟳</button>
        <input class="side-address" spellcheck="false" placeholder="adres, localhost:5173 albo ścieżka pliku" />
        <button data-a="save" class="side-save" hidden title="Zapisz (${shortcut("⌘S")})">zapisz</button>
        <button data-a="attach" title="Dodaj do aktywnej sesji (@ścieżka)">@</button>
        <button data-a="edit" title="Edytuj plik">✎</button>
        <button data-a="external" title="Otwórz w przeglądarce albo w domyślnym programie">↗</button>
        <button data-a="files" title="Pliki projektu (${shortcut("⌘F")})">☰</button>
        <button data-a="close" title="Zamknij podgląd">×</button>
      </div>
      <div class="side-body"><div class="side-view"><div class="side-empty dim">Kliknij link w terminalu (⌥-klik: przeglądarka) albo wpisz adres powyżej.</div></div></div>`;
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
      if (f === "attach") this.ctx.insert(mention(this.tree.project, this.folder));
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
      else if (a === "attach") this.ctx.insert(mention(this.tree.project, this.folder ?? this.editing?.path ?? this.address.value));
      else if (a === "edit") void (this.editing ? this.stopEditing() : this.edit());
      else if (a === "save") void this.save();
      else if (a === "external" && this.address.value) void api.openExternal(this.editing?.path ?? this.address.value).catch(this.fail);
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
    void listen<string>("preview-url", (e) => {
      if (document.activeElement !== this.address && !this.editing) this.address.value = e.payload;
      this.syncButtons();
    });
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
    } catch (err) {
      this.fail(err);
      throw err;
    } finally {
      this.syncButtons();
    }
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
        <button data-f="attach">Dodaj do sesji</button>
        <button data-f="terminal">Terminal tutaj</button>
        <button data-f="reveal">${REVEAL}</button>
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
    this.editBtn.textContent = this.editing ? "podgląd" : "✎";
    this.editBtn.title = this.editing ? "Zapisz i pokaż podgląd" : "Edytuj plik";
    this.saveBtn.hidden = !this.editing;
    this.saveBtn.classList.toggle("dirty", this.dirty());
  }

  private dirty(): boolean {
    return this.editing !== null && this.editing.area.value !== this.editing.text;
  }

  /** Edit the shown file in a plain editor in desk's own window. */
  private async edit(): Promise<void> {
    const path = this.address.value;
    let text: string;
    try {
      text = await api.readText(path);
    } catch (err) {
      return toast(`Edycja: ${err}`);
    }
    const area = document.createElement("textarea");
    area.className = "side-editor";
    area.spellcheck = false;
    area.value = text;
    area.addEventListener("input", () => this.syncButtons());
    area.addEventListener("keydown", (e) => {
      const mod = PLATFORM === "mac" ? e.metaKey : e.ctrlKey;
      if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        e.stopPropagation();
        void this.save();
      } else if (e.key === "Tab" && !e.shiftKey && !mod) {
        e.preventDefault();
        area.setRangeText("  ", area.selectionStart, area.selectionEnd, "end");
        this.syncButtons();
      }
    });
    this.editing = { path, text, area };
    this.view.replaceChildren(area);
    this.sync();
    this.syncButtons();
    area.focus();
  }

  private async save(): Promise<boolean> {
    const ed = this.editing;
    if (!ed) return true;
    const text = ed.area.value;
    try {
      await api.writeText(ed.path, text);
      ed.text = text;
      this.syncButtons();
      return true;
    } catch (err) {
      toast(`Zapis: ${err}`);
      return false;
    }
  }

  /** Back to the page: save first, then show it again, reloaded. */
  private async stopEditing(): Promise<boolean> {
    if (!(await this.save())) return false;
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
    if (this.created) return toast(`Podgląd: ${err}`);
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
