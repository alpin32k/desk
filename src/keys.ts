// App shortcuts, named after the Polish actions (⌘P projekty, ⌘Z zamknij, ...).
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
  | { type: "focus"; index: number }
  | { type: "fullscreen" }
  | { type: "exitFullscreen" }
  | { type: "settings" };

export interface KeyLike {
  key: string;
  metaKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
}

export function actionFor(e: KeyLike): Action | null {
  if (!e.metaKey || e.ctrlKey || e.altKey) return null;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  if (e.shiftKey) return key === "z" ? { type: "kill" } : null;
  if (key >= "1" && key <= "9") return { type: "focus", index: Number(key) - 1 };
  switch (key) {
    case "p": // projekty
      return { type: "palette" };
    case "n": // nowa sesja
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
    case "k": // kawa
      return { type: "coffee" };
    case "b": // blokada
      return { type: "lock" };
    case "r": // rozmowa
      return { type: "chat" };
    case "o": // opcje
      return { type: "settings" };
    default:
      return null;
  }
}

export const HINTS: [string, string][] = [
  ["⌘P", "projekty"],
  ["⌘N", "nowa"],
  ["⌘D", "druga sesja"],
  ["⌘R", "rozmowa"],
  ["⌘Z", "zamknij"],
  ["⌘⇧Z", "zabij"],
  ["⌘1-9", "skok"],
  ["⌘E", "ekran"],
  ["⌘K", "kawa"],
  ["⌘B", "blokada"],
  ["⌘O", "opcje"],
];
