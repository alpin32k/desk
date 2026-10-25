import { describe, expect, it } from "vitest";
import type { Project } from "./api";
import { ago, colorIndex, fuzzyScore, pickColor, PROJECT_COLORS, projectColor, rows, shortPath } from "./projects";

const p = (path: string, lastActiveMs: number, worktreeOf: string | null = null): Project => ({
  path,
  name: path.split("/").pop()!,
  lastActiveMs,
  isGit: true,
  worktreeOf,
  claudeSessions: 0,
});

const projects = [
  p("/h/mm-crm", 300),
  p("/h/gamer", 200),
  p("/h/mm-crm-wt/avatar-badge", 250, "/h/mm-crm"),
  p("/h/shopmm-frontend", 100),
  p("/h/shopmm-frontend-faq", 90, "/h/shopmm-frontend"),
  p("/h/orphan-wt", 400, "/h/not-listed"),
];

describe("fuzzyScore", () => {
  it("prefers substring, then subsequence, rejects non-matches", () => {
    expect(fuzzyScore("crm", "mm-crm")).toBeGreaterThan(fuzzyScore("mcm", "mm-crm"));
    expect(fuzzyScore("mcm", "mm-crm")).toBeGreaterThanOrEqual(0);
    expect(fuzzyScore("xyz", "mm-crm")).toBe(-1);
    expect(fuzzyScore("", "x")).toBe(0);
  });
});

describe("rows", () => {
  it("groups worktrees under their main repo when there is no query", () => {
    expect(rows(projects, "").map((r) => [r.project.name, r.parent?.name])).toEqual([
      ["mm-crm", undefined],
      ["avatar-badge", "mm-crm"],
      ["gamer", undefined],
      ["shopmm-frontend", undefined],
      ["shopmm-frontend-faq", "shopmm-frontend"],
      ["orphan-wt", undefined],
    ]);
  });

  it("ranks matches and finds worktrees by parent name", () => {
    const names = rows(projects, "crm").map((r) => r.project.name);
    expect(names[0]).toBe("mm-crm");
    expect(names).toContain("avatar-badge");
    expect(names).not.toContain("gamer");
  });
});

describe("formatting", () => {
  it("shortens home and formats age", () => {
    expect(shortPath("/Users/k/a", "/Users/k")).toBe("~/a");
    expect(shortPath("/Users/kx/a", "/Users/k")).toBe("/Users/kx/a");
    expect(ago(0)).toBe("");
    expect(ago(1000, 1000 + 2 * 3600_000)).toBe("2h");
    expect(ago(1000, 1000 + 3 * 86400_000)).toBe("3d");
  });
});

describe("project colors", () => {
  it("are stable, in range and overridable", () => {
    const i = colorIndex("/Users/k/mm-crm");
    expect(i).toBe(colorIndex("/Users/k/mm-crm"));
    expect(i).toBeGreaterThanOrEqual(0);
    expect(i).toBeLessThan(PROJECT_COLORS.length);
    expect(projectColor("/x", { "/x": 2 })).toBe(PROJECT_COLORS[2]);
    expect(projectColor("/x", { "/x": PROJECT_COLORS.length + 1 })).toBe(PROJECT_COLORS[1]);
  });

  it("gives open tiles different colors and keeps saved ones", () => {
    const taken: number[] = [];
    for (let n = 0; n < PROJECT_COLORS.length; n++) taken.push(pickColor(`/p/${n}`, {}, taken));
    expect(new Set(taken).size).toBe(PROJECT_COLORS.length);
    expect(pickColor("/a", {}, [])).toBe(colorIndex("/a"));
    expect(pickColor("/a", { "/a": 4 }, [4])).toBe(4);
    expect(pickColor("/a", {}, [colorIndex("/a")])).toBe((colorIndex("/a") + 1) % PROJECT_COLORS.length);
  });
});
