// App shortcuts. Anything not listed here goes to the focused terminal, so
// plain Esc still reaches Claude (it interrupts a turn there).
export type Action =
  | { type: "palette" }
  | { type: "new" }
  | { type: "close" }
  | { type: "kill" }
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
  if (e.shiftKey) return key === "w" ? { type: "kill" } : null;
  if (key >= "1" && key <= "9") return { type: "focus", index: Number(key) - 1 };
  switch (key) {
    case "k":
      return { type: "palette" };
    case "n":
      return { type: "new" };
    case "w":
      return { type: "close" };
    case "Enter":
      return { type: "fullscreen" };
    case "Escape":
      return { type: "exitFullscreen" };
    case ",":
      return { type: "settings" };
    default:
      return null;
  }
}

export const HINTS: [string, string][] = [
  ["⌘K", "projekty"],
  ["⌘N", "nowa sesja"],
  ["⌘W", "zamknij"],
  ["⌘⇧W", "zabij"],
  ["⌘1-9", "skok"],
  ["⌘↵", "pełny ekran"],
  ["⌘,", "opcje"],
];
