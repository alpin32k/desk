import { describe, expect, it } from "vitest";
import { DEFAULT_THEME, mix, terminalTheme, themeById, THEMES, uiColors } from "./themes";

const HEX = /^#[0-9a-f]{6}$/i;

describe("themes", () => {
  it("are complete, distinct and valid colors", () => {
    expect(THEMES.length).toBeGreaterThanOrEqual(12);
    expect(new Set(THEMES.map((t) => t.id)).size).toBe(THEMES.length);
    expect(THEMES.filter((t) => t.light).length).toBeGreaterThanOrEqual(3);
    for (const t of THEMES) {
      expect(t.ansi, t.id).toHaveLength(16);
      for (const c of [t.bg, t.fg, t.selection, ...t.ansi, ...Object.values(uiColors(t))]) expect(c, t.id).toMatch(HEX);
      expect(terminalTheme(t).brightWhite).toBe(t.ansi[15]);
    }
  });

  it("falls back to desk's own theme", () => {
    expect(themeById("nope").id).toBe(DEFAULT_THEME);
    expect(themeById(undefined).id).toBe(DEFAULT_THEME);
    expect(uiColors(themeById("desk")).panel).toBe("#101318");
  });

  it("mixes colors", () => {
    expect(mix("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mix("#102030", "#102030", 0.3)).toBe("#102030");
  });
});
