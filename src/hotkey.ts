// The dropdown's global shortcut (Options): recorded from a key press and
// stored as the accelerator Rust registers ("Super+Backquote"), shown as ⌘`.
import type { KeyLike } from "./keys";
import { PLATFORM, type Platform } from "./platform";

export const DEFAULT_HOTKEY = (platform: Platform = PLATFORM): string =>
  platform === "mac" ? "Super+Backquote" : "Control+Backquote";

const MODIFIER_CODES = /^(Meta|OS|Control|Alt|Shift)(Left|Right)?$/;

/** The accelerator for a key press, or null for a lone modifier or a key
 *  without ⌘, Ctrl or ⌥ (it would take typing away). F-keys may go alone. */
export function acceleratorFor(e: KeyLike): string | null {
  const code = e.code ?? "";
  if (!code || MODIFIER_CODES.test(code)) return null;
  const mods = [e.metaKey && "Super", e.ctrlKey && "Control", e.altKey && "Alt", e.shiftKey && "Shift"].filter(
    (m): m is string => !!m,
  );
  if (!e.metaKey && !e.ctrlKey && !e.altKey && !/^F\d+$/.test(code)) return null;
  return [...mods, code].join("+");
}

const KEY_LABELS: Record<string, string> = {
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Space: "Space",
  Enter: "↵",
  Tab: "Tab",
  Escape: "Esc",
  Backspace: "⌫",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

/** "Super+Shift+KeyD" as ⇧⌘D on macOS; elsewhere like desk's other
 *  shortcuts: Ctrl⇧P, Ctrl+`, Ctrl+Alt⇧D. */
export function hotkeyLabel(accelerator: string, platform: Platform = PLATFORM): string {
  const parts = accelerator.split("+");
  const code = parts.pop() ?? "";
  const key = KEY_LABELS[code] ?? code.replace(/^Key|^Digit/, "");
  const has = (m: string) => parts.some((p) => p.toLowerCase() === m.toLowerCase());
  if (platform === "mac") {
    // macOS order: ⌃⌥⇧⌘.
    return `${has("Control") ? "⌃" : ""}${has("Alt") ? "⌥" : ""}${has("Shift") ? "⇧" : ""}${has("Super") ? "⌘" : ""}${key}`;
  }
  const names = [has("Control") && "Ctrl", has("Alt") && "Alt", has("Super") && "Win"].filter(Boolean).join("+");
  if (has("Shift")) return `${names}⇧${key}`;
  return names ? `${names}+${key}` : key;
}
