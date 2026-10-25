import { describe, expect, it } from "vitest";
import { mention, normalize } from "./address";

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
});
