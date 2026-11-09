// The side panel's address bar.

/** What was typed in the address bar: a path, a URL, or a bare host. */
export function normalize(input: string): string {
  const s = input.trim();
  if (!s || /^[a-z][a-z0-9+.-]*:\/\//i.test(s) || s.startsWith("/") || s.startsWith("~") || /^[a-z]:[\\/]/i.test(s)) {
    return s;
  }
  return /^(localhost|127\.|\[::1\])/.test(s) ? `http://${s}` : `https://${s}`;
}

/** Shown rendered in the panel (Markdown, diagrams, tables, pages, images), not as text to edit. */
export function rendered(path: string): boolean {
  const ext = path.slice(path.lastIndexOf(".") + 1).toLowerCase();
  return path.includes(".") && RENDERED.has(ext);
}

const RENDERED = new Set(
  "md markdown mdx mmd mermaid csv tsv html htm pdf png jpg jpeg gif webp avif bmp ico svg".split(" "),
);

/** A local file or folder (not a web page). */
export function isLocal(address: string): boolean {
  return address.startsWith("/") || /^[a-z]:[\\/]/i.test(address);
}

/** `path` is `project` or inside it. */
export function inProject(project: string, path: string): boolean {
  const sep = path.includes("\\") ? "\\" : "/";
  return path === project || path.startsWith(project + sep);
}

/**
 * The side panel's view gives way to `project`'s folder card: a folder card
 * of another project always (the active tile moved), and when ⌘F / ⌘W open
 * the panel (`opening`) also an empty view or another project's file. A web
 * page stays. `shown`: the file or page shown, "" for none.
 */
export function staleView(project: string, view: { folder: string | null; shown: string }, opening: boolean): boolean {
  if (view.folder) return !inProject(project, view.folder);
  if (!opening) return false;
  return !view.shown || (isLocal(view.shown) && !inProject(project, view.shown));
}

/**
 * `@path` for Claude Code: relative to the project when inside it, quoted when
 * it has spaces. "" for a name with control characters or quotes: it is typed
 * into a terminal, where those could end the line or run something.
 */
export function mention(project: string | null, path: string): string {
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f\x7f"]/.test(path)) return "";
  let rel = path;
  if (project && inProject(project, path)) rel = path.slice(project.length + 1) || ".";
  return /\s/.test(rel) ? `@"${rel}" ` : `@${rel} `;
}

/**
 * Files dropped on a terminal, typed the way Terminal and iTerm type them: each
 * path with its shell characters escaped, so Claude Code takes an image as an
 * attachment and a shell takes the path as one word. "" when a name has a
 * control character, which could end the line.
 */
export function dropped(paths: string[]): string {
  // eslint-disable-next-line no-control-regex
  if (!paths.length || paths.some((p) => /[\x00-\x1f\x7f]/.test(p))) return "";
  return paths.map((p) => p.replace(/[ !"#$&'()*;<>?[\\\]^`{|}~]/g, "\\$&")).join(" ") + " ";
}
