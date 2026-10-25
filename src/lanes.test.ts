import { describe, expect, it } from "vitest";
import { add, flatten, group, groups, move, moveGroup, remove, restore, setLaneCount, shortestLane } from "./lanes";

// Session keys as in sessions.ts: "/a" is the main session, "codex#0:/a" another one of /a.
const of = (k: string) => k.slice(k.indexOf(":") + 1);

describe("lanes", () => {
  it("adds to the shortest lane and never twice", () => {
    let l = [["/a", "/b"], ["/c"], []];
    expect(shortestLane(l)).toBe(2);
    l = add(l, "/d");
    expect(l).toEqual([["/a", "/b"], ["/c"], ["/d"]]);
    expect(add(l, "/a")).toBe(l);
  });

  it("adds to a layout with no lanes", () => {
    expect(add([], "/a")).toEqual([["/a"]]);
  });

  it("moves within and across lanes", () => {
    const l = [["/a", "/b", "/c"], ["/d"]];
    expect(move(l, "/a", 0, 2)).toEqual([["/b", "/c", "/a"], ["/d"]]);
    expect(move(l, "/c", 1, 0)).toEqual([["/a", "/b"], ["/c", "/d"]]);
    expect(move(l, "/c", 1, 99)).toEqual([["/a", "/b"], ["/d", "/c"]]);
    expect(move(l, "/c", 5, 0)).toBe(l);
  });

  it("removes", () => {
    expect(remove([["/a"], ["/b"]], "/a")).toEqual([[], ["/b"]]);
  });

  it("shrinking lanes keeps every tile", () => {
    const l = [["/a"], ["/b", "/c"], ["/d"], ["/e"]];
    const two = setLaneCount(l, 2);
    expect(two).toHaveLength(2);
    expect(flatten(two).sort()).toEqual(["/a", "/b", "/c", "/d", "/e"]);
    expect(setLaneCount(l, 6)).toHaveLength(6);
    expect(setLaneCount(l, 0)).toHaveLength(1);
  });

  it("restores saved tiles plus autostart, in order", () => {
    const saved = [["/a"], ["/b"], [], []];
    expect(restore(saved, ["/b", "/x"], 4)).toEqual([["/a"], ["/b"], ["/x"], []]);
    expect(restore([], [], 4)).toEqual([[], [], [], []]);
  });

  it("adds a project's next session under its card", () => {
    const l = [["/a", "/b"], ["/c"]];
    expect(add(l, "claude#1:/a", of)).toEqual([["/a", "claude#1:/a", "/b"], ["/c"]]);
    expect(add(l, "codex#0:/c", of)).toEqual([["/a", "/b"], ["/c", "codex#0:/c"]]);
    expect(add(l, "/d", of)).toEqual([["/a", "/b"], ["/c", "/d"]]);
  });

  it("splits a lane into project cards", () => {
    expect(groups(["/a", "claude#1:/a", "/b", "codex#0:/b"], of)).toEqual([
      ["/a", "claude#1:/a"],
      ["/b", "codex#0:/b"],
    ]);
  });

  it("pulls scattered sessions of a project together", () => {
    const l = [["/a", "/b", "claude#1:/a"], ["codex#0:/a", "/c"]];
    expect(group(l, of)).toEqual([["/a", "claude#1:/a", "codex#0:/a", "/b"], ["/c"]]);
  });

  it("moves a whole project card", () => {
    const l = [["/a", "claude#1:/a", "/b"], ["/c"]];
    expect(moveGroup(l, "/a", of, 1, 1)).toEqual([["/b"], ["/c", "/a", "claude#1:/a"]]);
    expect(moveGroup(l, "/a", of, 0, 1)).toEqual([["/b", "/a", "claude#1:/a"], ["/c"]]);
    expect(moveGroup(l, "/x", of, 0, 0)).toBe(l);
  });
});
