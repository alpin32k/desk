import { PLATFORM, shortcut, type Platform } from "./platform";

// App shortcuts, named after the Polish actions (⌘P projekty, ⌘Z zamknij, ...);
// Ctrl+Shift instead of ⌘ on Windows and Linux (see platform.ts).
// Anything not listed here goes to the focused terminal, so plain Esc still
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

export function actionFor(e: KeyLike, platform: Platform = PLATFORM): Action | null {
  let key: string;
  if (platform === "mac") {
    if (!e.metaKey || e.ctrlKey || e.altKey) return null;
    key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (e.shiftKey) return key === "z" ? { type: "kill" } : null;
  } else {
    // Ctrl+Shift+letter; Ctrl+Shift+Backspace kills. Alt stays free for AltGr (ą, ż...).
    if (!e.ctrlKey || !e.shiftKey || e.metaKey || e.altKey) return null;
    const code = e.code ?? "";
    key = code.startsWith("Key") ? code.slice(3).toLowerCase() : code.startsWith("Digit") ? code.slice(5) : e.key;
    if (key === "Backspace") return { type: "kill" };
    if (key === "Escape") return null; // Ctrl+Shift+Esc is the Task Manager
  }
  if (key >= "1" && key <= "9" && key.length === 1) return { type: "focus", index: Number(key) - 1 };
  // Font size: ⌘= / ⌘+ bigger, ⌘- smaller, ⌘0 back to normal.
  if (key === "=" || key === "+" || key === "Equal") return { type: "font", step: 1 };
  if (key === "-" || key === "_" || key === "Minus") return { type: "font", step: -1 };
  if (key === "0") return { type: "font", step: 0 };
  switch (key) {
    case "p": // projekty
      return { type: "palette" };
    case "n": // nowy projekt
      return { type: "new" };
    case "d": // druga sesja w tym projekcie
      return { type: "sub" };
    case "z": // zamknij
      return { type: "close" };
    case "e": // ekran pełny
    case "Enter":
      return { type: "fullscreen" };
    case "Escape":
      return { type: "exitFullscreen" };
    case "j": // zwiń (⌘M zwija całe okno)
      return { type: "minimize" };
    case "k": // kawa
      return { type: "coffee" };
    case "b": // blokada
      return { type: "lock" };
    case "f": // pliki
      return { type: "files" };
    case "u": // uruchom
      return { type: "run" };
    case "t": // terminal
      return { type: "terminal" };
    case "r": // rozmowa
      return { type: "chat" };
    case "o": // opcje
      return { type: "settings" };
    case "w": // widok obok: podgląd i przeglądarka
      return { type: "preview" };
    default:
      return null;
  }
}

/** Footer hints as written for macOS; `hints()` gives them for the user's platform. */
export const HINTS: [string, string][] = [
  ["⌘P", "projekty"],
  ["⌘N", "nowy projekt"],
  ["⌘D", "druga sesja"],
  ["⌘T", "terminal"],
  ["⌘F", "pliki"],
  ["⌘U", "uruchom"],
  ["⌘R", "rozmowa"],
  ["⌘W", "podgląd"],
  ["⌘Z", "zamknij"],
  ["⌘⇧Z", "zabij"],
  ["⌘1-9", "skok"],
  ["⌘E", "ekran"],
  ["⌘J", "zwiń"],
  ["⌘K", "kawa"],
  ["⌘B", "blokada"],
  ["⌘O", "opcje"],
];

export function hints(platform: Platform = PLATFORM): [string, string][] {
  return HINTS.map(([k, label]) => [shortcut(k, platform), label]);
}
