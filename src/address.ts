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

/**
 * `@path` for Claude Code: relative to the project when inside it, quoted when
 * it has spaces. "" for a name with control characters or quotes: it is typed
 * into a terminal, where those could end the line or run something.
 */
export function mention(project: string | null, path: string): string {
  // eslint-disable-next-line no-control-regex
  if (/[\x00-\x1f\x7f"]/.test(path)) return "";
  const sep = path.includes("\\") ? "\\" : "/";
  let rel = path;
  if (project && (path === project || path.startsWith(project + sep))) rel = path.slice(project.length + 1) || ".";
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
