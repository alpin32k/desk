import { hotkeyLabel } from "./hotkey";
import { t, type MessageKey } from "./i18n";
import { PLATFORM, shortcut, type Platform } from "./platform";

// App shortcuts, named after the Polish actions (⌘P projekty, ⌘Q zamknij, ...);
// Ctrl+Shift instead of ⌘ on Windows and Linux (see platform.ts). Each one
// can be changed in Options → Skróty (layout.keys).
// Anything not bound goes to the focused terminal, so plain Esc still
// reaches Claude (it interrupts a turn there).
export type Action =
  | { type: "palette" }
  | { type: "new" }
  | { type: "close" }
  | { type: "kill" }
  | { type: "sub" }
  | { type: "coffee" }
  | { type: "lock" }
  | { type: "chat" }
  | { type: "sessions" }
  | { type: "terminal" }
  | { type: "files" }
  | { type: "run" }
  | { type: "focus"; index: number }
  | { type: "fullscreen" }
  | { type: "minimize" }
  | { type: "exitFullscreen" }
  | { type: "settings" }
  | { type: "quit" }
  | { type: "preview" }
  | { type: "font"; step: 1 | -1 | 0 };

export interface KeyLike {
  key: string;
  /** Physical key ("KeyP", "Digit1"); used off macOS, where Shift changes `key`. */
  code?: string;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
}

export type Group = "window" | "tiles" | "terminal" | "palette" | "preview";

export interface Binding {
  id: string;
  group: Group;
  action: Action;
  /** Accelerators on macOS ("Super+KeyP"); off macOS Super becomes Control+Shift. */
  mac: string[];
  /** Off macOS when not derived from `mac` ([] = none). */
  other?: string[];
}

/** Every shortcut that can be changed, in the order Options lists them. */
export const BINDINGS: Binding[] = [
  { id: "palette", group: "window", action: { type: "palette" }, mac: ["Super+KeyP"] },
  { id: "new", group: "window", action: { type: "new" }, mac: ["Super+KeyN"] },
  { id: "sub", group: "window", action: { type: "sub" }, mac: ["Super+KeyD"] },
  { id: "terminal", group: "window", action: { type: "terminal" }, mac: ["Super+KeyT"] },
  { id: "files", group: "window", action: { type: "files" }, mac: ["Super+KeyF"] },
  { id: "run", group: "window", action: { type: "run" }, mac: ["Super+KeyU"] },
  { id: "chat", group: "window", action: { type: "chat" }, mac: ["Super+KeyR"] },
  { id: "sessions", group: "window", action: { type: "sessions" }, mac: ["Super+KeyS"] },
  { id: "preview", group: "window", action: { type: "preview" }, mac: ["Super+KeyW"] },
  { id: "coffee", group: "window", action: { type: "coffee" }, mac: ["Super+KeyK"] },
  { id: "lock", group: "window", action: { type: "lock" }, mac: ["Super+KeyB"] },
  { id: "settings", group: "window", action: { type: "settings" }, mac: ["Super+KeyO"] },
  // None by default (⌘Q closes a tile): set one in Options, or use ⏻.
  { id: "quit", group: "window", action: { type: "quit" }, mac: [] },
  { id: "fullscreen", group: "tiles", action: { type: "fullscreen" }, mac: ["Super+KeyE", "Super+Enter"] },
  { id: "exitFullscreen", group: "tiles", action: { type: "exitFullscreen" }, mac: ["Super+Escape"], other: [] }, // Ctrl+Shift+Esc is the Task Manager
  { id: "minimize", group: "tiles", action: { type: "minimize" }, mac: ["Super+KeyJ"] },
  { id: "close", group: "tiles", action: { type: "close" }, mac: ["Super+KeyQ"] },
  { id: "kill", group: "tiles", action: { type: "kill" }, mac: ["Super+Shift+KeyQ"], other: ["Control+Shift+Backspace"] },
  { id: "fontUp", group: "terminal", action: { type: "font", step: 1 }, mac: ["Super+Equal"] },
  { id: "fontDown", group: "terminal", action: { type: "font", step: -1 }, mac: ["Super+Minus"] },
  { id: "fontReset", group: "terminal", action: { type: "font", step: 0 }, mac: ["Super+Digit0"] },
];

/** Shortcuts that stay as they are, written for macOS, by group. */
export const FIXED: { group: Group; keys: string; label: MessageKey }[] = [
  { group: "tiles", keys: "⌘1-9", label: "fixed.focus" },
  { group: "terminal", keys: "⌘C / ⌘V", label: "fixed.copyPaste" },
  { group: "palette", keys: "↵", label: "fixed.open" },
  { group: "palette", keys: "⌘↵", label: "fixed.autostart" },
  { group: "palette", keys: "⌘⌫", label: "fixed.kill" },
  { group: "palette", keys: "Tab", label: "fixed.agent" },
  { group: "preview", keys: "⌘S", label: "fixed.save" },
];

export const GROUPS: Group[] = ["window", "tiles", "terminal", "palette", "preview"];

/** What a binding does, in Options. */
export function bindingLabel(id: string): string {
  return t(`key.${id}` as MessageKey);
}

export function groupLabel(group: Group): string {
  return t(`group.${group}`);
}

/** Keys desk or the system already uses, which a binding must not take. */
const RESERVED: Record<Platform, Record<string, MessageKey>> = {
  mac: {
    "Super+KeyC": "reserved.copy",
    "Super+KeyV": "reserved.paste",
    "Super+KeyX": "reserved.cut",
    "Super+KeyZ": "reserved.undo",
    "Super+Shift+KeyZ": "reserved.redo",
    "Super+KeyH": "reserved.hide",
    "Super+KeyM": "reserved.minimize",
    "Super+Tab": "reserved.switchApps",
  },
  windows: { "Control+Shift+KeyC": "reserved.copy", "Control+Shift+KeyV": "reserved.paste", "Control+Shift+Escape": "reserved.taskManager" },
  linux: { "Control+Shift+KeyC": "reserved.copy", "Control+Shift+KeyV": "reserved.paste" },
};

let custom: Record<string, string> = {};

/** Shortcuts changed in Options (layout.keys), by binding id. */
export function setCustomKeys(keys: Record<string, string>): void {
  custom = { ...keys };
}

function defaults(b: Binding, platform: Platform): string[] {
  if (platform === "mac") return b.mac;
  return b.other ?? b.mac.map((a) => a.replace("Super", "Control+Shift"));
}

/** What triggers a binding now: the one set in Options, else its defaults. */
export function acceleratorsOf(b: Binding, platform: Platform = PLATFORM): string[] {
  return custom[b.id] ? [custom[b.id]] : defaults(b, platform);
}

/** "⌘P", or "Ctrl⇧P" off macOS; both defaults joined ("⌘E / ⌘↵"). */
export function keyText(id: string, platform: Platform = PLATFORM): string {
  const b = BINDINGS.find((x) => x.id === id);
  return b ? acceleratorsOf(b, platform).map((a) => hotkeyLabel(a, platform)).join(" / ") : "";
}

/** The binding (or reserved key) a new shortcut would clash with, if any. */
export function clashWith(id: string, accelerator: string, platform: Platform = PLATFORM): string | null {
  const reserved = RESERVED[platform][accelerator];
  if (reserved) return t(reserved);
  const focus = platform === "mac" ? /^Super\+Digit[1-9]$/ : /^Control\+Shift\+Digit[1-9]$/;
  if (focus.test(accelerator)) return t("fixed.focus");
  const other = BINDINGS.find((b) => b.id !== id && acceleratorsOf(b, platform).includes(accelerator));
  return other ? bindingLabel(other.id) : null;
}

const SYMBOLS: Record<string, string> = {
  "=": "Equal",
  "+": "Equal",
  "-": "Minus",
  _: "Minus",
  "`": "Backquote",
  "[": "BracketLeft",
  "]": "BracketRight",
  "\\": "Backslash",
  ";": "Semicolon",
  "'": "Quote",
  ",": "Comma",
  ".": "Period",
  "/": "Slash",
};

/** The pressed key as an accelerator ("Super+KeyP"). On macOS the key comes
 *  from `key`, so it follows the keyboard layout; elsewhere from `code`,
 *  because there Shift (always held) changes `key`. */
export function eventAccelerator(e: KeyLike, platform: Platform = PLATFORM): string {
  let key = e.code ?? "";
  if (platform === "mac") {
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (/^[a-z]$/.test(k)) key = `Key${k.toUpperCase()}`;
    else if (/^[0-9]$/.test(k)) key = `Digit${k}`;
    else if (SYMBOLS[k]) key = SYMBOLS[k];
    else if (k.length > 1) key = k;
  }
  const mods = [e.metaKey && "Super", e.ctrlKey && "Control", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
  return [...mods, key].join("+");
}

export function actionFor(e: KeyLike, platform: Platform = PLATFORM): Action | null {
  const acc = eventAccelerator(e, platform);
  const focus = (platform === "mac" ? /^Super\+Digit([1-9])$/ : /^Control\+Shift\+Digit([1-9])$/).exec(acc);
  if (focus) return { type: "focus", index: Number(focus[1]) - 1 };
  return BINDINGS.find((b) => acceleratorsOf(b, platform).includes(acc))?.action ?? null;
}

/** Footer hints, by binding id (their labels are "hint.<id>"). */
const HINTS = ["palette", "new", "sub", "terminal", "files", "run", "chat", "sessions", "preview", "close", "kill", "focus", "fullscreen", "minimize", "coffee", "lock", "settings"];

/** Footer hints for the user's platform and shortcuts (the first key of each). */
export function hints(platform: Platform = PLATFORM): [string, string][] {
  return HINTS.map((id) => {
    const label = t(`hint.${id}` as MessageKey);
    if (id === "focus") return [shortcut("⌘1-9", platform), label];
    const b = BINDINGS.find((x) => x.id === id)!;
    return [hotkeyLabel(acceleratorsOf(b, platform)[0], platform), label];
  });
}

/** Shortcuts written for macOS inside a text (a tooltip), as they are bound now. */
export function keysIn(text: string, platform: Platform = PLATFORM): string {
  return text.replace(/⌘⇧Q|⌘⌫|⌘↵|⌘[^\s)]*/g, (token) => {
    const mac = token.replace("⌘⇧", "⇧⌘"); // macOS writes ⇧ first
    const b = BINDINGS.find((x) => x.mac.some((a) => hotkeyLabel(a, "mac") === mac));
    return b ? hotkeyLabel(acceleratorsOf(b, platform)[0], platform) : shortcut(token, platform);
  });
}
