// Local files worth previewing when a path is printed in a terminal
// (Claude often writes mockups to an .html file and prints its path).
const FILE =
  /(?:^|[\s'"(`])((?:~|\/(?!\/)|[A-Za-z]:[\\/])[^\s'"()`]*\.(?:html?|svg|png|jpe?g|gif|webp|pdf))(?=$|[\s'"),.:;`])/gi;

export interface FileLink {
  path: string;
  /** 0-based column of the first character. */
  start: number;
  /** 0-based column after the last character. */
  end: number;
}

export function fileLinks(line: string, home = ""): FileLink[] {
  const out: FileLink[] = [];
  for (const m of line.matchAll(FILE)) {
    const raw = m[1];
    const start = (m.index ?? 0) + m[0].indexOf(raw);
    const path = raw.startsWith("~") ? (home ? home + raw.slice(1) : raw) : raw;
    if (path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path)) out.push({ path, start, end: start + raw.length });
  }
  return out;
}
