import { describe, expect, it } from "vitest";
import { dropped, inProject, isLocal, mention, normalize, rendered, staleView } from "./address";

describe("address bar", () => {
  it("turns what was typed into a link", () => {
    expect(normalize("localhost:5173")).toBe("http://localhost:5173");
    expect(normalize("127.0.0.1:8000/x")).toBe("http://127.0.0.1:8000/x");
    expect(normalize("example.com")).toBe("https://example.com");
    expect(normalize("http://a.test/")).toBe("http://a.test/");
    expect(normalize("/Users/me/README.md")).toBe("/Users/me/README.md");
    expect(normalize("~/notes.md")).toBe("~/notes.md");
    expect(normalize("C:\\docs\\a.md")).toBe("C:\\docs\\a.md");
  });

  it("tells documents shown rendered from text to edit", () => {
    expect(["/p/README.md", "/p/plan.MMD", "/p/data.csv", "/p/logo.svg", "/p/a.pdf"].map(rendered)).toEqual([true, true, true, true, true]);
    expect(["/p/src/app.ts", "/p/Cargo.toml", "/p/.env.example", "/p/Makefile", "/p/x.json"].map(rendered)).toEqual([false, false, false, false, false]);
  });

  it("mentions a path for Claude Code", () => {
    expect(mention("/p/app", "/p/app/src/a.ts")).toBe("@src/a.ts ");
    expect(mention("/p/app", "/p/app")).toBe("@. ");
    expect(mention("/p/app", "/p/application/x.ts")).toBe("@/p/application/x.ts ");
    expect(mention("/p/app", "/p/app/My Docs/a.md")).toBe('@"My Docs/a.md" ');
    expect(mention("C:\\p", "C:\\p\\src\\a.ts")).toBe("@src\\a.ts ");
    expect(mention(null, "/x/y")).toBe("@/x/y ");
    expect(mention("/p", "/p/a\nrm -rf ~\n.md")).toBe("");
    expect(mention("/p", "/p/\x1b[201~x")).toBe("");
    expect(mention("/p", '/p/a" b.md')).toBe("");
  });

  it("types dropped files as escaped paths", () => {
    expect(dropped(["/Users/me/shot.png"])).toBe("/Users/me/shot.png ");
    expect(dropped(["/Users/me/Zrzut ekranu (2).png", "/tmp/a&b.txt"])).toBe(
      "/Users/me/Zrzut\\ ekranu\\ \\(2\\).png /tmp/a\\&b.txt ",
    );
    expect(dropped(["/Users/me/żółw.jpg"])).toBe("/Users/me/żółw.jpg ");
    expect(dropped(["/tmp/a\nrm -rf ~\n.png"])).toBe("");
    expect(dropped([])).toBe("");
  });

  it("tells a path inside a project from one beside it", () => {
    expect(inProject("/p/app", "/p/app")).toBe(true);
    expect(inProject("/p/app", "/p/app/src/a.ts")).toBe(true);
    expect(inProject("/p/app", "/p/application/x.ts")).toBe(false);
    expect(inProject("/p/app", "/p/other")).toBe(false);
    expect(inProject("C:\\p", "C:\\p\\src\\a.ts")).toBe(true);
    expect(isLocal("/p/a.md")).toBe(true);
    expect(isLocal("C:\\p\\a.md")).toBe(true);
    expect(isLocal("https://example.com")).toBe(false);
  });

  it("lets the panel follow the active project", () => {
    const view = (folder: string | null, shown = "") => ({ folder, shown });
    // A tile of another project: its folder card gives way, whatever opened the panel.
    expect(staleView("/p/b", view("/p/a"), false)).toBe(true);
    expect(staleView("/p/b", view("/p/a"), true)).toBe(true);
    expect(staleView("/p/b", view("/p/b/src"), true)).toBe(false);
    // Switching tiles leaves a file or an empty panel alone...
    expect(staleView("/p/b", view(null, "/p/a/README.md"), false)).toBe(false);
    expect(staleView("/p/b", view(null), false)).toBe(false);
    // ...⌘F / ⌘W open on the project instead.
    expect(staleView("/p/b", view(null, "/p/a/README.md"), true)).toBe(true);
    expect(staleView("/p/b", view(null), true)).toBe(true);
    expect(staleView("/p/b", view(null, "/p/b/.env"), true)).toBe(false);
    // A web page stays.
    expect(staleView("/p/b", view(null, "https://example.com"), true)).toBe(false);
  });
});
