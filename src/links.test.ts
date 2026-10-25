import { describe, expect, it } from "vitest";
import { fileLinks } from "./links";

describe("fileLinks", () => {
  it("finds absolute paths to previewable files", () => {
    const line = "Saved mockup to /Users/k/app/mock.html, open it.";
    expect(fileLinks(line)).toEqual([{ path: "/Users/k/app/mock.html", start: 16, end: 38 }]);
    expect(fileLinks("see '/tmp/a.svg' and (/tmp/b.PNG)").map((l) => l.path)).toEqual(["/tmp/a.svg", "/tmp/b.PNG"]);
  });

  it("expands ~ when home is known and skips the rest", () => {
    expect(fileLinks("~/x/y.pdf", "/Users/k")[0].path).toBe("/Users/k/x/y.pdf");
    expect(fileLinks("~/x/y.pdf")).toEqual([]);
    expect(fileLinks("relative/mock.html src/app.ts https://x.dev/a.html")).toEqual([]);
  });
});
