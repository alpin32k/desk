// One tile: header (claude · project · branch · state) and a live xterm.js
// terminal attached to the project's tmux session through a Rust PTY.
import { Channel } from "@tauri-apps/api/core";
import { ClipboardAddon } from "@xterm/addon-clipboard";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { preview } from "./side";
import { api, type HookState } from "./api";
import { stripProbeReplies } from "./input";
import { PLATFORM, shortcuts } from "./platform";
import { actionFor } from "./keys";
import { fileLinks } from "./links";
import { agentLabel, keyOf, type Spec } from "./sessions";

export type TileState = Exclude<HookState, "exited"> | "shell" | "starting";

export type TileAction = "close" | "kill" | "fullscreen" | "focus" | "color" | "attached" | "sub" | "terminal" | "branch" | "run";

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

/** State shown on the tile: the hook state while claude runs, "shell" after it exits. */
export function displayState(hook: HookState | undefined): TileState {
  if (hook === "exited") return "shell";
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
  readonly key: string;
  readonly path: string;
  readonly agent: string;
  readonly instance: number;
  readonly name: string;
  slug: string;
  ptyId: number | null = null;
  branch: string | null = null;
  state: TileState = "starting";
  index = 0;
  /** Shown inside a project card with other sessions: the card names the project. */
  grouped = false;
  /** For a ⌘U session: the command it runs ("pnpm dev"). */
  detail: string | null = null;
  /** A ⌘R conversation, labelled "rozmowa" instead of the agent. */
  chat = false;

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
    spec: Spec,
    slug: string,
    private onAction: (tile: Tile, action: TileAction) => void,
  ) {
    this.key = keyOf(spec);
    this.path = spec.path;
    this.agent = spec.agent;
    this.instance = spec.instance;
    this.slug = slug;
    this.name = spec.path.split(/[\\/]/).filter(Boolean).pop() ?? spec.path;

    this.el = document.createElement("section");
    this.el.className = "tile state-starting";
    this.el.dataset.key = this.key;
    this.el.innerHTML = shortcuts(`
      <header draggable="true">
        <span class="dot" data-a="color" title="Kolor projektu (kliknij, aby zmienić)"></span>
        <span class="badge"></span>
        <span class="title"></span>
        <span class="actions">
          <button data-a="sub" title="Druga sesja w tym projekcie (⌘D)">+</button>
          <button data-a="terminal" title="Terminal w tym projekcie (⌘T)">›_</button>
          <button data-a="run" title="Uruchom w tym projekcie, np. dev serwer (⌘U)">▶</button>
          <button data-a="fullscreen" title="Ekran pełny (⌘E)">⤢</button>
          <button data-a="close" title="Zamknij kafelek, sesja zostaje (⌘Z)">×</button>
          <button data-a="kill" title="Zabij sesję (⌘⇧Z)">⏻</button>
        </span>
      </header>
      <div class="term"></div>`);
    this.badge = this.el.querySelector(".badge")!;
    this.title = this.el.querySelector(".title")!;
    this.body = this.el.querySelector(".term")!;

    const header = this.el.querySelector("header")!;
    header.addEventListener("dragstart", (e) => {
      e.dataTransfer?.setData("text/x-desk-key", this.key);
      e.dataTransfer!.effectAllowed = "move";
      this.el.classList.add("dragging");
    });
    header.addEventListener("dragend", () => this.el.classList.remove("dragging"));
    header.addEventListener("dblclick", () => this.onAction(this, "fullscreen"));
    header.addEventListener("click", (e) => {
      const a = (e.target as HTMLElement).closest<HTMLElement>("[data-a]")?.dataset.a as TileAction | undefined;
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
    // Click a link: preview inside desk; Option-click: system browser.
    const openLink = (e: MouseEvent, uri: string) => {
      const p = e.altKey ? api.openExternal(uri) : preview(uri);
      p.catch((err) => this.term.write(`\r\n\x1b[33m${err}\x1b[0m\r\n`));
    };
    this.term.loadAddon(new WebLinksAddon(openLink));
    this.term.registerLinkProvider({
      provideLinks: (y, callback) => {
        const line = this.term.buffer.active.getLine(y - 1)?.translateToString(true) ?? "";
        callback(
          fileLinks(line).map((l) => ({
            range: { start: { x: l.start + 1, y }, end: { x: l.end, y } },
            text: l.path,
            activate: (e: MouseEvent, text: string) => openLink(e, text),
          })),
        );
      },
    });
    this.term.attachCustomKeyEventHandler((e) => {
      if (actionFor(e)) return false; // app shortcut, handled by the window listener
      if (PLATFORM !== "mac" && e.type === "keydown" && e.ctrlKey && e.shiftKey && !e.altKey) {
        // Ctrl+Shift+C / V: copy and paste (plain Ctrl+C goes to the program).
        if (e.code === "KeyC") {
          const text = this.term.getSelection();
          if (text) void navigator.clipboard.writeText(text);
          return false;
        }
        if (e.code === "KeyV") {
          void navigator.clipboard.readText().then((t) => t && this.term.paste(t));
          return false;
        }
      }
      if (e.type === "keydown" && e.key === "Enter" && e.shiftKey && !e.metaKey && !e.ctrlKey) {
        // Shift+Enter = newline in Claude Code (sent as Meta+Enter).
        if (this.ptyId !== null) void api.ptyWrite(this.ptyId, "\x1b\r");
        return false;
      }
      return true;
    });
    this.term.onData((raw) => {
      const d = stripProbeReplies(raw);
      if (d && this.ptyId !== null) void api.ptyWrite(this.ptyId, d);
    });

    this.resizeObs = new ResizeObserver(() => this.queueFit());
    this.render();
  }

  /** Open the terminal and attach to tmux; call once the element is in the DOM. */
  async start(): Promise<void> {
    this.term.open(this.body);
    // The textarea exists only after open(); clicking a terminal makes its tile active.
    this.term.textarea?.addEventListener("focus", () => this.onAction(this, "focus"));
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
    this.ptyId = null;
    try {
      this.ptyId = await api.ptyAttach(this.slug, this.term.cols, this.term.rows, ch);
    } catch (err) {
      this.term.write(`\r\n\x1b[31mNie udało się podłączyć do sesji: ${err}\x1b[0m\r\n⌘Z zamyka kafelek.\r\n`);
      return;
    }
    if (this.disposed) void api.ptyClose(this.ptyId);
    else this.onAction(this, "attached");
  }

  focus(): void {
    this.term.focus();
  }

  /** What the mini card in the side rail shows: the last lines with text. */
  snapshot(n = 3): string[] {
    const b = this.term.buffer.active;
    const out: string[] = [];
    for (let y = b.baseY + b.cursorY; y >= 0 && out.length < n; y--) {
      const line = b.getLine(y)?.translateToString(true).trim() ?? "";
      // Skip empty lines and box borders (Claude Code's input frame).
      if (line.replace(/[\s─━│┃╭╮╰╯┌┐└┘]/g, "").length > 1) out.unshift(line);
    }
    return out;
  }

  /** Header text for the mini card: label, project, state. */
  summary(): { label: string; name: string; state: string } {
    return {
      label: this.label(),
      name: this.name,
      state: this.agent === "claude" ? STATE_LABEL[this.state] : "",
    };
  }

  /** Project color: left edge, name and dot in the header. */
  setColor(color: string): void {
    this.el.style.setProperty("--proj", color);
  }

  setActive(active: boolean): void {
    this.el.classList.toggle("active", active);
  }

  update(patch: { branch?: string | null; state?: TileState; index?: number; grouped?: boolean }): void {
    const next = {
      branch: patch.branch === undefined ? this.branch : patch.branch,
      state: patch.state ?? this.state,
      index: patch.index ?? this.index,
      grouped: patch.grouped ?? this.grouped,
    };
    if (
      next.branch === this.branch &&
      next.state === this.state &&
      next.index === this.index &&
      next.grouped === this.grouped
    )
      return;
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
    this.el.classList.toggle("grouped", this.grouped);
    const parts = [
      this.grouped
        ? `<b>${escapeHtml(this.label())}</b>`
        : `<span class="agent">${escapeHtml(this.label())}</span>`,
      this.grouped ? "" : `<b>${escapeHtml(this.name)}</b>`,
      this.branch && !this.grouped
        ? `<span class="branch" data-a="branch" title="Zmień branch">${escapeHtml(this.branch)}</span>`
        : "",
      // Only Claude Code reports working/waiting/idle through desk's hooks.
      this.agent === "claude" ? `<span class="state">${STATE_LABEL[this.state]}</span>` : "",
    ].filter(Boolean);
    this.title.innerHTML = parts.join(`<span class="sep"> · </span>`);
    this.title.title = `${this.agent} · ${this.path}`;
  }

  private label(): string {
    if (this.detail) return `▶ ${this.detail}`;
    if (this.chat) return this.instance > 0 ? `rozmowa #${this.instance + 1}` : "rozmowa";
    return agentLabel(this);
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
