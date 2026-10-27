// Local files worth previewing when a path is printed in a terminal
// (Claude often writes mockups to an .html file, or a plan to a .md file,
// and prints its path): pages, images, PDF and the documents the panel
// renders. Absolute paths (~ too), or relative ones with a folder in them
// (docs/plan.md, ./notes.md), which the tile resolves in its project.
const EXT = String.raw`\.(?:html?|svg|png|jpe?g|gif|webp|pdf|md|markdown|mmd|json|csv|tsv|txt)`;
const SEG = String.raw`[^\s'"()\`\\/:]+`;
const ABSOLUTE = String.raw`(?:~|\/(?!\/)|[A-Za-z]:[\\/])[^\s'"()\`]*`;
const RELATIVE = String.raw`(?:\.\.?\/)*(?:${SEG}\/)*${SEG}`;
const FILE = new RegExp(
  String.raw`(?:^|[\s'"(\`])((?:${ABSOLUTE}|${RELATIVE})${EXT})(?=$|[\s'"),.:;\`])`,
  "gi",
);

export interface FileLink {
  path: string;
  /** Relative to the tile's project (docs/plan.md), not absolute. */
  relative: boolean;
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
    const absolute = path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path);
    // A bare name (README.md) is too often just a word; a relative path needs a folder.
    const relative = !absolute && !path.startsWith("~") && path.includes("/");
    if (absolute || relative) out.push({ path, relative, start, end: start + raw.length });
  }
  return out;
}

/** A terminal row: its text, and whether the terminal wrapped it onto from the row above. */
export interface Row {
  text: string;
  wrapped: boolean;
}

/** Where a character sits on screen: index into the rows, 0-based column. */
export interface Cell {
  row: number;
  col: number;
}

export interface RowLink {
  path: string;
  relative: boolean;
  start: Cell;
  /** Just after the last character. */
  end: Cell;
}

/**
 * Does `next` carry on the path that `row` ends with? The terminal wrapping
 * the row says so; otherwise Claude Code (Ink) broke a long path itself:
 * the row is full to (almost) the right edge, ends in a path fragment, and
 * the next row goes on, indented like the message.
 */
function continues(row: Row, next: Row, cols: number): boolean {
  if (next.wrapped) return true;
  const text = row.text.trimEnd();
  if (text.length < cols - 3 || !next.text.trim()) return false;
  const last = text.slice(text.search(/\S+$/));
  return /[\\/]/.test(last);
}

/** Previewable file paths in the line that row `at` belongs to, even when it is wrapped over several rows. */
export function rowLinks(rows: Row[], at: number, cols: number, home = ""): RowLink[] {
  let first = at;
  while (first > 0 && continues(rows[first - 1], rows[first], cols)) first--;
  let last = at;
  while (last + 1 < rows.length && continues(rows[last], rows[last + 1], cols)) last++;
  let text = "";
  const cells: Cell[] = [];
  for (let r = first; r <= last; r++) {
    let row = rows[r].text;
    let col = 0;
    if (r > first && !rows[r].wrapped) {
      // An Ink break: drop the indent of the continued row and the trailing blanks before it.
      col = row.length - row.trimStart().length;
      row = row.trimStart();
      text = text.trimEnd();
      cells.length = text.length;
    }
    for (let i = 0; i < row.length; i++) cells.push({ row: r, col: col + i });
    text += row;
  }
  return fileLinks(text, home)
    .map((l) => {
      const lastCell = cells[l.end - 1];
      return { path: l.path, relative: l.relative, start: cells[l.start], end: { row: lastCell.row, col: lastCell.col + 1 } };
    })
    .filter((l) => l.start.row <= at && l.end.row >= at);
}
