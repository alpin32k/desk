import { describe, expect, it } from "vitest";
import { ancestors, buildTree, filterPaths, findNode, itemsLabel } from "./filetree";

const FILES = ["src/main.ts", "README.md", ".github/workflows/check.yml", "src/app.ts", "src-tauri/src/lib.rs", "docs/a10.md", "docs/a9.md"];

describe("file tree", () => {
  it("puts folders first, then files, by name", () => {
    const root = buildTree(FILES);
    expect(root.children.map((c) => c.name)).toEqual([".github", "docs", "src", "src-tauri", "README.md"]);
    expect(findNode(root, "src")!.children.map((c) => c.name)).toEqual(["app.ts", "main.ts"]);
    expect(findNode(root, "docs")!.children.map((c) => c.name)).toEqual(["a9.md", "a10.md"]);
    expect(findNode(root, "src-tauri/src/lib.rs")).toMatchObject({ dir: false, path: "src-tauri/src/lib.rs" });
    expect(findNode(root, "nope/x")).toBeNull();
    expect(findNode(root, "")).toBe(root);
  });

  it("filters by every word, file-name hits first", () => {
    expect(filterPaths(FILES, "lib")).toEqual(["src-tauri/src/lib.rs"]);
    expect(filterPaths(FILES, "src ts")).toEqual(["src/app.ts", "src/main.ts"]);
    expect(filterPaths(FILES, "src lib")).toEqual(["src-tauri/src/lib.rs"]);
    expect(filterPaths(FILES, "md")[0]).toBe("README.md");
    expect(filterPaths(FILES, "  ")).toEqual([]);
  });

  it("lists the folders above a path", () => {
    expect(ancestors("a/b/c.ts")).toEqual(["a", "a/b"]);
    expect(ancestors("c.ts")).toEqual([]);
  });

  it("counts items in Polish", () => {
    expect([1, 2, 4, 5, 12, 13, 22, 25, 104].map(itemsLabel)).toEqual([
      "1 element",
      "2 elementy",
      "4 elementy",
      "5 elementów",
      "12 elementów",
      "13 elementów",
      "22 elementy",
      "25 elementów",
      "104 elementy",
    ]);
  });
});
