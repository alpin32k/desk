// One tile: header (claude · project · branch · state) and a live xterm.js
// terminal attached to the project's tmux session through a Rust PTY.
import { Channel } from "@tauri-apps/api/core";
import { ClipboardAddon } from "@xterm/addon-clipboard";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { api, type HookState } from "./api";
import { actionFor } from "./keys";

export type TileState = HookState | "shell" | "starting";

export type TileAction = "close" | "kill" | "fullscreen" | "focus";

const THEME = {
  background: "#0b0d10",
  foreground: "#c9d1d9",
  cursor: "#e6edf3",
  cursorAccent: "#0b0d10",
  selectionBackground: "#264f78",
  black: "#1b1f24",
  red: "#ff7b72",
  green: "#7ee787",
  yellow: "#e3b341",
  blue: "#79c0ff",
  magenta: "#d2a8ff",
  cyan: "#56d4dd",
  white: "#c9d1d9",
  brightBlack: "#6e7681",
  brightRed: "#ffa198",
  brightGreen: "#aff5b4",
  brightYellow: "#f8e3a1",
  brightBlue: "#a5d6ff",
  brightMagenta: "#e2c5ff",
  brightCyan: "#b3f0ff",
  brightWhite: "#f0f6fc",
};

const SHELLS = new Set(["zsh", "bash", "fish", "sh", "-zsh", "-bash", "login"]);

/** State shown on the tile: the hook state while claude runs, "shell" after it exits. */
export function displayState(hook: HookState | undefined, command: string | null | undefined): TileState {
  if (command && SHELLS.has(command)) return "shell";
  return hook ?? "starting";
}

const STATE_LABEL: Record<TileState, string> = {
  working: "pracuje",
  waiting: "czeka na ciebie",
  idle: "bezczynna",
  shell: "shell",
  starting: "start",
};

let webglBudget = 12; // WebKit caps live WebGL contexts; the rest use the DOM renderer.

export class Tile {
  readonly el: HTMLElement;
  readonly path: string;
  readonly name: string;
  slug: string;
  ptyId: number | null = null;
  branch: string | null = null;
  state: TileState = "starting";
  index = 0;

  private term: Terminal;
  private fit = new FitAddon();
  private body: HTMLElement;
  private title: HTMLElement;
  private badge: HTMLElement;
  private resizeObs: ResizeObserver;
  private fitQueued = false;
  private webgl: WebglAddon | null = null;
  private disposed = false;

  constructor(
    path: string,
    slug: string,
    private onAction: (tile: Tile, action: TileAction) => void,
  ) {
    this.path = path;
    this.slug = slug;
    this.name = path.split("/").filter(Boolean).pop() ?? path;

    this.el = document.createElement("section");
    this.el.className = "tile state-starting";
    this.el.dataset.path = path;
    this.el.innerHTML = `
      <header draggable="true">
        <span class="badge"></span>
        <span class="title"></span>
        <span class="actions">
          <button data-a="fullscreen" title="Pełny ekran (⌘↵)">⤢</button>
          <button data-a="close" title="Zamknij kafelek, sesja zostaje (⌘W)">×</button>
          <button data-a="kill" title="Zabij sesję (⌘⇧W)">⏻</button>
        </span>
      </header>
      <div class="term"></div>`;
    this.badge = this.el.querySelector(".badge")!;
    this.title = this.el.querySelector(".title")!;
    this.body = this.el.querySelector(".term")!;

    const header = this.el.querySelector("header")!;
    header.addEventListener("dragstart", (e) => {
      e.dataTransfer?.setData("text/x-desk-path", path);
      e.dataTransfer!.effectAllowed = "move";
      this.el.classList.add("dragging");
    });
    header.addEventListener("dragend", () => this.el.classList.remove("dragging"));
    header.addEventListener("dblclick", () => this.onAction(this, "fullscreen"));
    header.addEventListener("click", (e) => {
      const a = (e.target as HTMLElement).closest("button")?.dataset.a as TileAction | undefined;
      this.onAction(this, a ?? "focus");
    });

    this.term = new Terminal({
      fontFamily: '"JetBrainsMono Nerd Font Mono", "JetBrains Mono", ui-monospace, "SF Mono", Menlo, monospace',
      fontSize: 11.5,
      lineHeight: 1.08,
      theme: THEME,
      cursorBlink: false,
      allowProposedApi: true,
      scrollback: 1000,
      macOptionIsMeta: false, // keep Option for Polish letters
      macOptionClickForcesSelection: true,
      drawBoldTextInBrightColors: false,
    });
    this.term.loadAddon(this.fit);
    this.term.loadAddon(new ClipboardAddon());
    this.term.attachCustomKeyEventHandler((e) => {
      if (actionFor(e)) return false; // app shortcut, handled by the window listener
      if (e.type === "keydown" && e.key === "Enter" && e.shiftKey && !e.metaKey && !e.ctrlKey) {
        // Shift+Enter = newline in Claude Code (sent as Meta+Enter).
        if (this.ptyId !== null) void api.ptyWrite(this.ptyId, "\x1b\r");
        return false;
      }
      return true;
    });
    this.term.onData((d) => {
      if (this.ptyId !== null) void api.ptyWrite(this.ptyId, d);
    });
    this.term.textarea?.addEventListener("focus", () => this.onAction(this, "focus"));

    this.resizeObs = new ResizeObserver(() => this.queueFit());
    this.render();
  }

  /** Open the terminal and attach to tmux; call once the element is in the DOM. */
  async start(): Promise<void> {
    this.term.open(this.body);
    if (webglBudget > 0) {
      try {
        const gl = new WebglAddon();
        gl.onContextLoss(() => {
          gl.dispose();
          this.webgl = null;
        });
        this.term.loadAddon(gl);
        this.webgl = gl;
        webglBudget--;
      } catch {
        this.webgl = null;
      }
    }
    this.resizeObs.observe(this.body);
    this.fitNow();
    await this.attach();
  }

  async attach(): Promise<void> {
    const ch = new Channel<ArrayBuffer>();
    ch.onmessage = (buf) => this.term.write(new Uint8Array(buf));
    this.ptyId = await api.ptyAttach(this.slug, this.term.cols, this.term.rows, ch);
    if (this.disposed) void api.ptyClose(this.ptyId);
  }

  focus(): void {
    this.term.focus();
  }

  setActive(active: boolean): void {
    this.el.classList.toggle("active", active);
  }

  update(patch: { branch?: string | null; state?: TileState; index?: number }): void {
    const next = {
      branch: patch.branch === undefined ? this.branch : patch.branch,
      state: patch.state ?? this.state,
      index: patch.index ?? this.index,
    };
    if (next.branch === this.branch && next.state === this.state && next.index === this.index) return;
    Object.assign(this, next);
    this.render();
  }

  queueFit(): void {
    if (this.fitQueued) return;
    this.fitQueued = true;
    requestAnimationFrame(() => {
      this.fitQueued = false;
      this.fitNow();
    });
  }

  private fitNow(): void {
    if (this.disposed || !this.body.isConnected || this.body.clientWidth < 20 || this.body.clientHeight < 20) return;
    const { cols, rows } = this.term;
    this.fit.fit();
    if (this.ptyId !== null && (cols !== this.term.cols || rows !== this.term.rows)) {
      void api.ptyResize(this.ptyId, this.term.cols, this.term.rows);
    }
  }

  private render(): void {
    this.el.className = this.el.className.replace(/\bstate-\S+/, `state-${this.state}`);
    this.badge.textContent = this.index > 0 && this.index < 10 ? String(this.index) : "";
    const parts = [
      `<span class="dim">claude</span>`,
      `<b>${escapeHtml(this.name)}</b>`,
      this.branch ? `<span class="branch">${escapeHtml(this.branch)}</span>` : "",
      `<span class="state">${STATE_LABEL[this.state]}</span>`,
    ].filter(Boolean);
    this.title.innerHTML = parts.join(`<span class="sep"> · </span>`);
    this.title.title = this.path;
  }

  /** Detach from tmux (the session keeps running) and free the terminal. */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.resizeObs.disconnect();
    if (this.ptyId !== null) void api.ptyClose(this.ptyId);
    if (this.webgl) webglBudget++;
    this.term.dispose();
    this.el.remove();
  }
}

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}
