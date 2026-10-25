import { describe, expect, it } from "vitest";
import { normalize } from "./address";

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
});
