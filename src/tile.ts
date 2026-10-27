// One tile: header (claude · project · branch · state) and a live xterm.js
// terminal attached to the project's tmux session through a Rust PTY.
import { Channel } from "@tauri-apps/api/core";
import { ClipboardAddon } from "@xterm/addon-clipboard";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { mayWrite } from "./clip";
import { toast } from "./ui";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal, type ITheme } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { openOutside, preview, previewFile } from "./side";
import { api, type HookState } from "./api";
import { stripProbeReplies } from "./input";
import { DEFAULT_THEME, terminalTheme, themeById } from "./themes";
import { PLATFORM, shortcuts } from "./platform";
import { actionFor } from "./keys";
import { rowLinks, type Row } from "./links";
import { agentLabel, keyOf, type Spec } from "./sessions";

/** Rows looked at above and below a hovered one for a wrapped path. */
const LINK_ROWS = 4;

export type TileState = Exclude<HookState, "exited"> | "shell" | "starting";

export type TileAction = "close" | "kill" | "fullscreen" | "minimize" | "focus" | "color" | "attached" | "sub" | "terminal" | "branch" | "run";

/** Terminal colors of the chosen theme (Options); new tiles start with them. */
let termTheme: ITheme = terminalTheme(themeById(DEFAULT_THEME));

export function setTerminalTheme(t: ITheme): void {
  termTheme = t;
}

export const DEFAULT_FONT_SIZE = 11.5;
let fontSize = DEFAULT_FONT_SIZE;

/** Font size for new tiles (Options, ⌘= / ⌘-). */
export function setTerminalFontSize(px: number): void {
  fontSize = px;
}

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
  /** Name given by hand (✎ or right-click on the title), shown instead of the agent. */
  private customName: string | null = null;
  /** The user named the tile; "" clears the name. */
  onRename: (name: string) => void = () => {};
  /** A ⌘R conversation, labelled "rozmowa" instead of the agent. */
  chat = false;

  private term: Terminal;
  private fit = new FitAddon();
  /** Your last real click or key in this tile (clipboard writes: clip.ts). */
  private lastInput = 0;
  private released: (e: MouseEvent) => void = () => undefined;
  private body: HTMLElement;
  private title: HTMLElement;
  private badge: HTMLElement;
  private resizeObs: ResizeObserver;
  private fitQueued = false;
  /** Folded to its header, which shows the last line of output. */
  private minimized = false;
  private peek: HTMLElement;
  private peekQueued = false;
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
        <span class="peek"></span>
        <span class="actions">
          <button data-a="sub" title="Druga sesja w tym projekcie (⌘D)">+</button>
          <button data-a="terminal" title="Terminal w tym projekcie (⌘T)">›_</button>
          <button data-a="run" title="Uruchom w tym projekcie, np. dev serwer (⌘U)">▶</button>
          <button data-a="rename" title="Nazwij kafelek (albo prawy klik na tytule)">✎</button>
          <button data-a="minimize" title="Zwiń: zostaje nagłówek z ostatnią linią (⌘J)">–</button>
          <button data-a="fullscreen" title="Ekran pełny (⌘E)">⤢</button>
          <button data-a="close" title="Zamknij kafelek, sesja zostaje (⌘Z)">×</button>
          <button data-a="kill" title="Zabij sesję (⌘⇧Z)">⏻</button>
        </span>
      </header>
      <div class="term"></div>`);
    this.badge = this.el.querySelector(".badge")!;
    this.title = this.el.querySelector(".title")!;
    this.body = this.el.querySelector(".term")!;
    this.peek = this.el.querySelector(".peek")!;

    const header = this.el.querySelector("header")!;
    header.addEventListener("dragstart", (e) => {
      e.dataTransfer?.setData("text/x-desk-key", this.key);
      e.dataTransfer!.effectAllowed = "move";
      this.el.classList.add("dragging");
    });
    header.addEventListener("dragend", () => this.el.classList.remove("dragging"));
    header.addEventListener("dblclick", () => this.onAction(this, "fullscreen"));
    header.addEventListener("click", (e) => {
      if (this.renaming) return;
      const a = (e.target as HTMLElement).closest<HTMLElement>("[data-a]")?.dataset.a;
      if (a === "rename") return this.rename();
      this.onAction(this, (a as TileAction | undefined) ?? "focus");
    });
    this.title.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      this.rename();
    });

    this.term = new Terminal({
      fontFamily: '"JetBrainsMono Nerd Font Mono", "JetBrains Mono", ui-monospace, "SF Mono", Menlo, monospace',
      fontSize,
      lineHeight: 1.08,
      theme: termTheme,
      cursorBlink: false,
      allowProposedApi: true,
      scrollback: 1000,
      macOptionIsMeta: false, // keep Option for Polish letters
      macOptionClickForcesSelection: true,
      drawBoldTextInBrightColors: false,
    });
    this.term.loadAddon(this.fit);
    // OSC 52 goes to the system clipboard natively, only right after your own
    // click or key here (clip.ts). Programs asking to read the clipboard get nothing.
    this.term.loadAddon(
      new ClipboardAddon(undefined, {
        readText: () => "",
        writeText: (_sel, text) => {
          if (!mayWrite(text, this.lastInput, Date.now())) {
            if (text) toast(`${this.name}: program chciał zmienić schowek bez Twojej akcji, zablokowano`);
            return;
          }
          return writeText(text).then(() => toast("Skopiowano do schowka"));
        },
      }),
    );
    // A selection starts with a press here and may end anywhere (tmux copies on release).
    let pressed = false;
    // Only real (trusted) DOM events count: not data the terminal sends by itself
    // (cursor and focus reports a program can ask for).
    this.body.addEventListener(
      "mousedown",
      (e) => {
        if (!e.isTrusted) return;
        pressed = true;
        this.lastInput = Date.now();
      },
      true,
    );
    this.released = (e: MouseEvent) => {
      if (!e.isTrusted || !pressed) return;
      pressed = false;
      this.lastInput = Date.now();
    };
    window.addEventListener("mouseup", this.released, true);
    // Click a link: preview inside desk; Option-click: Brave / Chrome / system browser.
    const openLink = (e: MouseEvent, uri: string, relative = false) => {
      // A relative path (docs/plan.md) is a file of this tile's project.
      const p = relative
        ? e.altKey
          ? openOutside(`${this.path.replace(/[\\/]+$/, "")}/${uri}`)
          : previewFile(this.path, uri)
        : e.altKey
          ? openOutside(uri)
          : preview(uri);
      p.catch((err) => this.term.write(`\r\n\x1b[33m${err}\x1b[0m\r\n`));
    };
    this.term.loadAddon(new WebLinksAddon(openLink));
    this.term.registerLinkProvider({
      provideLinks: (y, callback) => {
        // A few rows around this one: a long path can be wrapped over several.
        const buf = this.term.buffer.active;
        const top = Math.max(0, y - 1 - LINK_ROWS);
        const rows: Row[] = [];
        for (let i = top; i < Math.min(buf.length, y + LINK_ROWS); i++) {
          const line = buf.getLine(i);
          rows.push({ text: line?.translateToString(true) ?? "", wrapped: line?.isWrapped ?? false });
        }
        callback(
          rowLinks(rows, y - 1 - top, this.term.cols).map((l) => ({
            range: { start: { x: l.start.col + 1, y: top + l.start.row + 1 }, end: { x: l.end.col, y: top + l.end.row + 1 } },
            text: l.path,
            activate: (e: MouseEvent, text: string) => openLink(e, text, l.relative),
          })),
        );
      },
    });
    this.term.attachCustomKeyEventHandler((e) => {
      if (e.type === "keydown" && e.isTrusted) this.lastInput = Date.now();
      if (actionFor(e)) return false; // app shortcut, handled by the window listener
      // Copy and paste: ⌘C / ⌘V, or Ctrl+Shift+C / V off macOS (plain Ctrl+C goes to the program).
      const clip =
        PLATFORM === "mac" ? e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey : e.ctrlKey && e.shiftKey && !e.metaKey && !e.altKey;
      if (clip && (e.code === "KeyC" || e.code === "KeyV")) {
        if (e.type !== "keydown") return false;
        e.preventDefault(); // not the menu's paste as well
        if (e.code === "KeyC") {
          const text = this.term.getSelection();
          if (text) void writeText(text);
        } else {
          void readText()
            .then((t) => t && this.term.paste(t))
            .catch(() => undefined);
        }
        return false;
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
    // No system menu (Cut, Spelling, Speech...) over a terminal: right click
    // belongs to the program in it.
    this.body.addEventListener("contextmenu", (e) => e.preventDefault());
    // The textarea exists only after open(); clicking a terminal makes its tile active.
    this.term.textarea?.addEventListener("focus", () => this.onAction(this, "focus"));
    this.term.onWriteParsed(() => {
      if (this.minimized) this.queuePeek();
    });
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

  /** Type `text` into the session as if pasted (no Enter). */
  type(text: string): void {
    if (this.ptyId !== null) void api.ptyWrite(this.ptyId, text);
    this.focus();
  }

  setTheme(t: ITheme): void {
    this.term.options.theme = t;
  }

  setFontSize(px: number): void {
    this.term.options.fontSize = px;
    this.fitNow();
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

  get isMinimized(): boolean {
    return this.minimized;
  }

  /** Fold the tile to its header (the session keeps running), or unfold it. */
  setMinimized(on: boolean): void {
    if (on === this.minimized) return;
    this.minimized = on;
    this.el.classList.toggle("minimized", on);
    const btn = this.el.querySelector<HTMLElement>('[data-a="minimize"]')!;
    btn.textContent = on ? "▢" : "–";
    btn.title = shortcuts(on ? "Rozwiń (⌘J)" : "Zwiń: zostaje nagłówek z ostatnią linią (⌘J)");
    if (on) this.queuePeek();
    else {
      this.peek.textContent = "";
      this.queueFit();
    }
  }

  private queuePeek(): void {
    if (this.peekQueued) return;
    this.peekQueued = true;
    requestAnimationFrame(() => {
      this.peekQueued = false;
      if (this.minimized && !this.disposed) this.peek.textContent = this.snapshot(1)[0] ?? "";
    });
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
        : `<span class="${this.customName ? "named" : "agent"}">${escapeHtml(this.label())}</span>`,
      this.grouped ? "" : `<b>${escapeHtml(this.name)}</b>`,
      this.branch && !this.grouped
        ? `<span class="branch" data-a="branch" title="Zmień branch">${escapeHtml(this.branch)}</span>`
        : "",
      // Only Claude Code reports working/waiting/idle through desk's hooks.
      this.agent === "claude" ? `<span class="state">${STATE_LABEL[this.state]}</span>` : "",
    ].filter(Boolean);
    if (!this.renaming) this.title.innerHTML = parts.join(`<span class="sep"> · </span>`);
    this.title.title = `${this.agent} · ${this.path}`;
  }

  private renaming = false;

  setName(name: string | null): void {
    this.customName = name || null;
    this.render();
  }

  /** Edit the tile's name in its header: Enter or leaving the field saves, Esc cancels. */
  rename(): void {
    if (this.renaming) return;
    this.renaming = true;
    const header = this.el.querySelector("header")!;
    header.draggable = false;
    const input = document.createElement("input");
    input.className = "rename";
    input.spellcheck = false;
    input.placeholder = "nazwa kafelka, np. logi API";
    input.value = this.customName ?? "";
    // Selecting text in the field must not start fullscreen or a drag.
    for (const type of ["click", "dblclick", "mousedown"]) input.addEventListener(type, (e) => e.stopPropagation());
    let done = false;
    const finish = (save: boolean) => {
      if (done) return;
      done = true;
      this.renaming = false;
      header.draggable = true;
      if (save) this.onRename(input.value.trim());
      this.render();
      this.focus();
    };
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") finish(true);
      else if (e.key === "Escape") finish(false);
    });
    input.addEventListener("blur", () => finish(true));
    this.title.replaceChildren(input);
    input.focus();
    input.select();
  }

  private label(): string {
    if (this.customName) return this.customName;
    if (this.detail) return `▶ ${this.detail}`;
    if (this.chat) return this.instance > 0 ? `rozmowa #${this.instance + 1}` : "rozmowa";
    return agentLabel(this);
  }

  /** Detach from tmux (the session keeps running) and free the terminal. */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.resizeObs.disconnect();
    window.removeEventListener("mouseup", this.released, true);
    if (this.ptyId !== null) void api.ptyClose(this.ptyId);
    if (this.webgl) webglBudget++;
    this.term.dispose();
    this.el.remove();
  }
}

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}
