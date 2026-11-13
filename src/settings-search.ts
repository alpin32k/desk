// The search field in Options: which rows a query leaves on screen.

/** Lower case, no diacritics (ł too), single spaces, trimmed. */
export function normalize(text: string): string {
  return text
    .toLowerCase()
    .replace(/ł/g, "l") // NFD leaves it whole
    .normalize("NFD")
    .replace(/\p{M}+/gu, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** One thing Options can show or hide: a line, a shortcut, or a whole section. */
export interface SearchItem {
  /** Id of its group in the sidebar. */
  group: string;
  /** Header of its section; "" when it has none. */
  section: string;
  text: string;
}

export interface SearchResult {
  /** Same order as the items. */
  show: boolean[];
  /** Shown items per group id; empty when there is no query. */
  counts: Record<string, number>;
  /** The query after normalize(); "" = not searching. */
  query: string;
}

/**
 * An item stays when the query is found in its text, in its section's header
 * or in its group's name (`groupNames`: group id -> name in the sidebar).
 */
export function search(items: SearchItem[], groupNames: Record<string, string>, query: string): SearchResult {
  const q = normalize(query);
  const counts: Record<string, number> = {};
  if (!q) return { show: items.map(() => true), counts, query: q };
  const show = items.map((item) => {
    const hit = [item.text, item.section, groupNames[item.group] ?? ""].some((s) => normalize(s).includes(q));
    if (hit) counts[item.group] = (counts[item.group] ?? 0) + 1;
    return hit;
  });
  return { show, counts, query: q };
}
