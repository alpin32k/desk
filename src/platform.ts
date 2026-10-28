// macOS uses ⌘ for desk's shortcuts. On Windows and Linux ⌘ does not exist
// and Ctrl+letter belongs to the terminal (Ctrl+C, Ctrl+R...), so desk uses
// Ctrl+Shift+letter there, like Windows Terminal and most Linux terminals.
export type Platform = "mac" | "windows" | "linux";

export function detect(ua: string): Platform {
  if (/Mac|iPhone|iPad/.test(ua)) return "mac";
  if (/Windows/.test(ua)) return "windows";
  return "linux";
}

export const PLATFORM: Platform = detect(typeof navigator === "undefined" ? "" : navigator.userAgent);

/** Shortcut text written for macOS ("⌘P", "⌘⇧Q") as the user's platform types it. */
export function shortcut(mac: string, platform: Platform = PLATFORM): string {
  if (platform === "mac") return mac;
  return mac.replace("⌘⇧Q", "Ctrl⇧⌫").replace("⌘⌫", "Ctrl⌫").replace("⌘↵", "Ctrl↵").replace(/⌘/g, "Ctrl⇧");
}

/** Every ⌘-shortcut inside a longer text, e.g. a tooltip. */
export function shortcuts(text: string, platform: Platform = PLATFORM): string {
  return platform === "mac" ? text : text.replace(/⌘⇧Q|⌘⌫|⌘↵|⌘[^\s)]*/g, (m) => shortcut(m, platform));
}
