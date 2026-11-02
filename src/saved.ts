// Claude conversations saved under a name. Naming a Claude tile saves the
// conversation in it; ⌘S lists the saved ones of the project first, however
// old, and one picked comes back in a new tile with its name. The name stays
// with the conversation: after /clear the tile goes back to its default name.

import type { Conversation } from "./api";

/** By project path, then by conversation id: the name. */
export type Saved = Record<string, Record<string, string>>;

/** `saved` with conversation `id` of project `path` under `name`, or forgotten for an empty one. */
export function setSaved(saved: Saved, path: string, id: string, name: string): Saved {
  const project = { ...saved[path] };
  const trimmed = name.trim();
  if (trimmed) project[id] = trimmed;
  else delete project[id];
  const next = { ...saved, [path]: project };
  if (!Object.keys(project).length) delete next[path];
  return next;
}

/** What a Claude tile named `title` (or not) does about conversation `id` now in it. */
export interface TileSync {
  /** Save the conversation under the tile's name. */
  save?: string;
  /** The tile's new name (null: back to the default). */
  title?: string | null;
}

export function syncTile(saved: Saved, path: string, title: string | null, id: string | null): TileSync {
  if (!id) return {};
  const project = saved[path] ?? {};
  const name = project[id];
  // A saved conversation (also one picked up with /resume) shows its name.
  if (name !== undefined) return name === title ? {} : { title: name };
  if (!title) return {};
  // The name belongs to another conversation: /clear or a new one in the tile.
  if (Object.entries(project).some(([other, n]) => other !== id && n === title)) return { title: null };
  // Named before Claude had a conversation, or before desk saved them.
  return { save: title };
}

/** A row of ⌘S: a conversation, with its name when saved. */
export interface ConversationRow {
  c: Conversation;
  name: string | null;
}

/** Saved ones first, then the rest; each newest first, as `found` comes. */
export function conversationRows(found: Conversation[], names: Record<string, string>): ConversationRow[] {
  const rows = found.map((c) => ({ c, name: names[c.id] ?? null }));
  return [...rows.filter((r) => r.name !== null), ...rows.filter((r) => r.name === null)];
}

/** Every typed word is in the name, Claude's title, the first message or the branch. */
export function matchesRow(row: ConversationRow, query: string): boolean {
  const text = [row.name, row.c.title, row.c.prompt, row.c.branch].filter(Boolean).join(" ").toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => text.includes(w));
}
