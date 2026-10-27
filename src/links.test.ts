import { describe, expect, it } from "vitest";
import { fileLinks, rowLinks, type Row } from "./links";

describe("fileLinks", () => {
  it("finds absolute paths to previewable files", () => {
    const line = "Saved mockup to /Users/k/app/mock.html, open it.";
    expect(fileLinks(line)).toEqual([{ path: "/Users/k/app/mock.html", start: 16, end: 38, relative: false }]);
    expect(fileLinks("see '/tmp/a.svg' and (/tmp/b.PNG)").map((l) => l.path)).toEqual(["/tmp/a.svg", "/tmp/b.PNG"]);
  });

  it("expands ~ when home is known and skips the rest", () => {
    expect(fileLinks("~/x/y.pdf", "/Users/k")[0].path).toBe("/Users/k/x/y.pdf");
    expect(fileLinks("~/x/y.pdf")).toEqual([]);
    expect(fileLinks("src/app.ts https://x.dev/a.html mock.html")).toEqual([]);
  });

  it("finds paths relative to the project, and documents the panel shows", () => {
    const line = "Plan jest gotowy: docs/superpowers/plans/2026-10-27-mockups.md. Ma 14 zadań";
    expect(fileLinks(line)).toEqual([
      { path: "docs/superpowers/plans/2026-10-27-mockups.md", start: 18, end: 62, relative: true },
    ]);
    expect(fileLinks("./notes.md, ../x/data.CSV i out/a.json:3").map((l) => [l.path, l.relative])).toEqual([
      ["./notes.md", true],
      ["../x/data.CSV", true],
      ["out/a.json", true],
    ]);
    expect(fileLinks("/Users/k/plan.md")).toEqual([{ path: "/Users/k/plan.md", start: 0, end: 16, relative: false }]);
    // Bare names and URLs stay text.
    expect(fileLinks("README.md https://x.dev/docs/a.md")).toEqual([]);
  });

  it("finds Windows paths", () => {
    const line = "Zapisałem C:\\Users\\jan\\mock.html i C:/tmp/a.png";
    expect(fileLinks(line).map((l) => l.path)).toEqual(["C:\\Users\\jan\\mock.html", "C:/tmp/a.png"]);
  });

  it("ignores network paths", () => {
    expect(fileLinks("see //attacker/share/x.html and \\\\host\\s\\y.html")).toEqual([]);
  });

  it("joins a path Claude Code broke over two rows", () => {
    const cols = 40;
    const rows: Row[] = [
      { text: "● Otworzyłem Ci makietę w Chrome:", wrapped: false },
      { text: "  /private/tmp/claude-501/scratch/4893", wrapped: false },
      { text: "  d61c-c2c8/chmurka/na-zywo.html", wrapped: false },
      { text: "", wrapped: false },
    ];
    expect(rows[1].text.length).toBeGreaterThanOrEqual(cols - 3);
    const link = {
      path: "/private/tmp/claude-501/scratch/4893d61c-c2c8/chmurka/na-zywo.html",
      relative: false,
      start: { row: 1, col: 2 },
      end: { row: 2, col: 32 },
    };
    expect(rowLinks(rows, 1, cols)).toEqual([link]);
    expect(rowLinks(rows, 2, cols)).toEqual([link]);
    expect(rowLinks(rows, 0, cols)).toEqual([]);
  });

  it("joins rows the terminal wrapped and leaves short rows apart", () => {
    const rows: Row[] = [
      { text: "zapisane w /tmp/a/b", wrapped: false },
      { text: "/mock.html gotowe", wrapped: true },
    ];
    expect(rowLinks(rows, 1, 19).map((l) => [l.path, l.start, l.end])).toEqual([
      ["/tmp/a/b/mock.html", { row: 0, col: 11 }, { row: 1, col: 10 }],
    ]);
    // A short row ending in a folder is not continued by the next one.
    const apart: Row[] = [
      { text: "  folder /tmp/a/", wrapped: false },
      { text: "  b.html", wrapped: false },
    ];
    expect(rowLinks(apart, 1, 80)).toEqual([]);
  });
});
