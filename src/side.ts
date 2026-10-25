// Side panel: links, local files, Markdown and diagrams next to the terminals.
// The page itself is a second native webview (no access to desk's commands)
// that Rust keeps over `.side-view`; this file draws the bar above it and
// tells Rust where that box is.

import { listen } from "@tauri-apps/api/event";
import { api, type PreviewBounds } from "./api";
import { toast } from "./ui";
import { normalize } from "./address";

const WIDTH_KEY = "desk.sideWidth";

/** Open a link or a local file in the panel (from a terminal link, ⌘F...). */
export function preview(link: string): Promise<void> {
  return Side.current ? Side.current.open(link) : Promise.reject("panel podglądu nie jest gotowy");
}

/** A project file in the panel (⌘F, ⌥↵). */
export function previewFile(path: string, file: string): Promise<void> {
  return Side.current ? Side.current.open(file, path) : Promise.reject("panel podglądu nie jest gotowy");
}

export class Side {
  static current: Side | null = null;
  readonly el: HTMLElement;
  private view: HTMLElement;
  private address: HTMLInputElement;
  /** A webview exists in Rust (it may be hidden). */
  private created = false;
  private covered = false;

  constructor(
    private onLayout: () => void,
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
        <button data-a="external" title="Otwórz w przeglądarce">↗</button>
        <button data-a="close" title="Zamknij podgląd">×</button>
      </div>
      <div class="side-view"><div class="side-empty dim">Kliknij link w terminalu (⌥-klik: przeglądarka) albo wpisz adres powyżej.</div></div>`;
    this.view = this.el.querySelector(".side-view")!;
    this.address = this.el.querySelector(".side-address")!;
    try {
      const w = Number(localStorage.getItem(WIDTH_KEY));
      if (w > 0) this.el.style.width = `${w}px`;
    } catch {
      // storage unavailable: default width
    }

    this.el.querySelector(".side-head")!.addEventListener("click", (e) => {
      const a = (e.target as HTMLElement).closest<HTMLElement>("[data-a]")?.dataset.a;
      if (a === "close") this.close();
      else if (a === "external" && this.address.value) void api.openExternal(this.address.value).catch(this.fail);
      else if ((a === "back" || a === "forward" || a === "reload") && this.created) void api.previewNav(a).catch(this.fail);
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
      if (document.activeElement !== this.address) this.address.value = e.payload;
    });
  }

  get isOpen(): boolean {
    return !this.el.hidden;
  }

  /** `link`, or with `project` a file relative to that project. */
  async open(link: string, project?: string): Promise<void> {
    if (!link.trim()) return;
    this.show();
    try {
      this.address.value = project
        ? await api.previewFile(project, link, this.bounds())
        : await api.openPreview(link, this.bounds());
      this.created = true;
      this.view.classList.add("live");
      // A new page shows at once: hide it again if a dialog is open.
      if (this.covered) void api.previewVisible(false);
    } catch (err) {
      this.fail(err);
      throw err;
    }
  }

  /** ⌘W: show or hide; an empty panel starts at its address bar. */
  toggle(): void {
    if (this.isOpen) return this.close();
    this.show();
    if (this.created) void api.previewVisible(!this.covered);
    else this.address.focus();
  }

  close(): void {
    if (!this.isOpen) return;
    this.el.hidden = true;
    if (this.created) void api.previewVisible(false);
    this.onLayout();
  }

  private show(): void {
    if (this.isOpen) return;
    this.el.hidden = false;
    this.onLayout();
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
    if (this.created && this.isOpen) void api.previewVisible(!covered);
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
