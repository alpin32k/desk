import { hotkeyLabel } from "./hotkey";
import { PLATFORM, shortcut, type Platform } from "./platform";

// App shortcuts, named after the Polish actions (⌘P projekty, ⌘Z zamknij, ...);
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
  | { type: "terminal" }
  | { type: "files" }
  | { type: "run" }
  | { type: "focus"; index: number }
  | { type: "fullscreen" }
  | { type: "minimize" }
  | { type: "exitFullscreen" }
  | { type: "settings" }
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

export interface Binding {
  id: string;
  group: string;
  /** What it does, in Options. */
  label: string;
  action: Action;
  /** Accelerators on macOS ("Super+KeyP"); off macOS Super becomes Control+Shift. */
  mac: string[];
  /** Off macOS when not derived from `mac` ([] = none). */
  other?: string[];
}

/** Every shortcut that can be changed, in the order Options lists them. */
export const BINDINGS: Binding[] = [
  { id: "palette", group: "Okno", label: "projekty (paleta)", action: { type: "palette" }, mac: ["Super+KeyP"] },
  { id: "new", group: "Okno", label: "nowy projekt (folder + opcjonalnie git init)", action: { type: "new" }, mac: ["Super+KeyN"] },
  { id: "sub", group: "Okno", label: "druga sesja w tym projekcie", action: { type: "sub" }, mac: ["Super+KeyD"] },
  { id: "terminal", group: "Okno", label: "terminal (zwykły shell) w projekcie aktywnego kafelka", action: { type: "terminal" }, mac: ["Super+KeyT"] },
  { id: "files", group: "Okno", label: "pliki projektu", action: { type: "files" }, mac: ["Super+KeyF"] },
  { id: "run", group: "Okno", label: "uruchom (dev serwer, testy...)", action: { type: "run" }, mac: ["Super+KeyU"] },
  { id: "chat", group: "Okno", label: "rozmowa (czat bez projektu)", action: { type: "chat" }, mac: ["Super+KeyR"] },
  { id: "preview", group: "Okno", label: "podgląd obok (pliki, Markdown, diagramy, strony)", action: { type: "preview" }, mac: ["Super+KeyW"] },
  { id: "coffee", group: "Okno", label: "kawa", action: { type: "coffee" }, mac: ["Super+KeyK"] },
  { id: "lock", group: "Okno", label: "blokada", action: { type: "lock" }, mac: ["Super+KeyB"] },
  { id: "settings", group: "Okno", label: "opcje", action: { type: "settings" }, mac: ["Super+KeyO"] },
  { id: "fullscreen", group: "Kafelki", label: "ekran pełny kafelka", action: { type: "fullscreen" }, mac: ["Super+KeyE", "Super+Enter"] },
  { id: "exitFullscreen", group: "Kafelki", label: "wyjście z ekranu pełnego", action: { type: "exitFullscreen" }, mac: ["Super+Escape"], other: [] }, // Ctrl+Shift+Esc is the Task Manager
  { id: "minimize", group: "Kafelki", label: "zwiń kafelek do nagłówka i rozwiń", action: { type: "minimize" }, mac: ["Super+KeyJ"] },
  { id: "close", group: "Kafelki", label: "zamknij kafelek (sesja zostaje)", action: { type: "close" }, mac: ["Super+KeyZ"] },
  { id: "kill", group: "Kafelki", label: "zabij sesję", action: { type: "kill" }, mac: ["Super+Shift+KeyZ"], other: ["Control+Shift+Backspace"] },
  { id: "fontUp", group: "Terminal", label: "czcionka większa", action: { type: "font", step: 1 }, mac: ["Super+Equal"] },
  { id: "fontDown", group: "Terminal", label: "czcionka mniejsza", action: { type: "font", step: -1 }, mac: ["Super+Minus"] },
  { id: "fontReset", group: "Terminal", label: "czcionka zwykła", action: { type: "font", step: 0 }, mac: ["Super+Digit0"] },
];

/** Shortcuts that stay as they are, written for macOS, by group. */
export const FIXED: { group: string; keys: string; label: string }[] = [
  { group: "Kafelki", keys: "⌘1-9", label: "skok do kafelka" },
  { group: "Terminal", keys: "⌘C / ⌘V", label: "kopiuj zaznaczenie / wklej" },
  { group: "Paleta", keys: "↵", label: "otwiera" },
  { group: "Paleta", keys: "⌘↵", label: "włącza lub wyłącza autostart" },
  { group: "Paleta", keys: "⌘⌫", label: "zabija sesję" },
  { group: "Paleta", keys: "Tab", label: "wybiera agenta" },
  { group: "Podgląd", keys: "⌘S", label: "zapisz plik w edytorze" },
];

export const GROUPS = ["Okno", "Kafelki", "Terminal", "Paleta", "Podgląd"];

/** Keys desk or the system already uses, which a binding must not take. */
const RESERVED: Record<Platform, Record<string, string>> = {
  mac: {
    "Super+KeyC": "kopiuj",
    "Super+KeyV": "wklej",
    "Super+KeyX": "wytnij",
    "Super+KeyQ": "zamknij desk",
    "Super+KeyH": "ukryj desk",
    "Super+KeyM": "zwiń okno",
    "Super+Tab": "przełączanie aplikacji",
  },
  windows: { "Control+Shift+KeyC": "kopiuj", "Control+Shift+KeyV": "wklej", "Control+Shift+Escape": "Menedżer zadań" },
  linux: { "Control+Shift+KeyC": "kopiuj", "Control+Shift+KeyV": "wklej" },
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
  if (reserved) return reserved;
  const focus = platform === "mac" ? /^Super\+Digit[1-9]$/ : /^Control\+Shift\+Digit[1-9]$/;
  if (focus.test(accelerator)) return "skok do kafelka";
  const other = BINDINGS.find((b) => b.id !== id && acceleratorsOf(b, platform).includes(accelerator));
  return other ? other.label : null;
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

/** Footer hints: binding id and short label. */
const HINTS: [string, string][] = [
  ["palette", "projekty"],
  ["new", "nowy projekt"],
  ["sub", "druga sesja"],
  ["terminal", "terminal"],
  ["files", "pliki"],
  ["run", "uruchom"],
  ["chat", "rozmowa"],
  ["preview", "podgląd"],
  ["close", "zamknij"],
  ["kill", "zabij"],
  ["focus", "skok"],
  ["fullscreen", "ekran"],
  ["minimize", "zwiń"],
  ["coffee", "kawa"],
  ["lock", "blokada"],
  ["settings", "opcje"],
];

/** Footer hints for the user's platform and shortcuts (the first key of each). */
export function hints(platform: Platform = PLATFORM): [string, string][] {
  return HINTS.map(([id, label]) => {
    if (id === "focus") return [shortcut("⌘1-9", platform), label];
    const b = BINDINGS.find((x) => x.id === id)!;
    return [hotkeyLabel(acceleratorsOf(b, platform)[0], platform), label];
  });
}

/** Shortcuts written for macOS inside a text (a tooltip), as they are bound now. */
export function keysIn(text: string, platform: Platform = PLATFORM): string {
  return text.replace(/⌘⇧Z|⌘⌫|⌘↵|⌘[^\s)]*/g, (token) => {
    const mac = token.replace("⌘⇧", "⇧⌘"); // macOS writes ⇧ first
    const b = BINDINGS.find((x) => x.mac.some((a) => hotkeyLabel(a, "mac") === mac));
    return b ? hotkeyLabel(acceleratorsOf(b, platform)[0], platform) : shortcut(token, platform);
  });
}
