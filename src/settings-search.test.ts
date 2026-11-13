import { describe, expect, it } from "vitest";
import { normalize, search } from "./settings-search";

const names = { term: "Terminal i linki", look: "Wygląd", power: "Zasilanie" };
const items = [
  { group: "term", section: "Linki", text: "Klik w link otwiera" },
  { group: "term", section: "Linki", text: "Przeglądarka poza desk" },
  { group: "term", section: "Terminal", text: "Menu tmuxa pod prawym klikiem" },
  { group: "look", section: "Motyw", text: "desk Cursor Dark" },
  { group: "power", section: "Kawa", text: "Wyłącza się sama przy przegrzaniu" },
];

describe("normalize", () => {
  it("drops case, diacritics and extra spaces", () => {
    expect(normalize("  Przeglądarka   POZA desk ")).toBe("przegladarka poza desk");
    expect(normalize("Łącze żółć")).toBe("lacze zolc");
  });
});

describe("search", () => {
  it("no query shows everything and counts nothing", () => {
    expect(search(items, names, "")).toEqual({ show: [true, true, true, true, true], counts: {}, query: "" });
    expect(search(items, names, "   ").query).toBe("");
  });

  it("matches the item text without diacritics", () => {
    const r = search(items, names, "przegladarka");
    expect(r.show).toEqual([false, true, false, false, false]);
    expect(r.counts).toEqual({ term: 1 });
  });

  it("a section header shows all its items", () => {
    // "Terminal i linki" is the group name too.
    expect(search(items, names, "linki").show).toEqual([true, true, true, false, false]);
    expect(search(items, names, "motyw").show).toEqual([false, false, false, true, false]);
  });

  it("a group name shows the whole group", () => {
    const r = search(items, names, "zasil");
    expect(r.show).toEqual([false, false, false, false, true]);
    expect(r.counts).toEqual({ power: 1 });
  });

  it("nothing found", () => {
    const r = search(items, names, "xyz");
    expect(r.show.some(Boolean)).toBe(false);
    expect(r.counts).toEqual({});
  });
});
