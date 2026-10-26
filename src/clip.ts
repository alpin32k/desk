// Programs in a terminal may set the clipboard (OSC 52: tmux copies a mouse
// selection this way, Claude Code's /copy too), but only right after you did
// something in that tile, so text printed by whatever runs there (a file being
// shown, a page Claude read) cannot slip into the clipboard behind your back.

/** How long after your click or key a program may set the clipboard. */
export const WRITE_WINDOW_MS = 3000;
export const MAX_CLIP = 1_000_000;

export function mayWrite(text: string, lastInputMs: number, nowMs: number): boolean {
  return text.length > 0 && text.length <= MAX_CLIP && nowMs - lastInputMs <= WRITE_WINDOW_MS;
}
