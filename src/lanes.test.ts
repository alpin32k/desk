import { describe, expect, it } from "vitest";
import {
  add,
  autoColumns,
  autoLanes,
  distribute,
  flatten,
  group,
  groups,
  move,
  moveGroup,
  moveGroupToNewLane,
  moveInGroup,
  place,
  prune,
  remove,
  restore,
  setLaneCount,
  shortestLane,
  split,
} from "./lanes";

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

  it("balances sessions across columns in order", () => {
    expect(distribute([1, 1, 1, 1], 2)).toEqual([2, 2]);
    expect(distribute([3, 1, 1, 1], 2)).toEqual([1, 3]);
    expect(distribute([1, 1], 5)).toEqual([1, 1]);
    expect(distribute([2, 1, 1], 3)).toEqual([1, 1, 1]);
  });

  it("picks columns from what is open and the window size", () => {
    const W = 2000;
    const H = 1100;
    expect(autoColumns([1], W, H)).toBe(1);
    expect(autoColumns([1, 1], W, H)).toBe(2);
    expect(autoColumns([1, 1, 1], W, H)).toBe(3);
    expect(autoColumns([2, 1], W, H)).toBe(2); // a project with two sessions stays one card
    expect(autoColumns([1, 1, 1, 1, 1, 1], W, H)).toBe(4);
    expect(autoColumns([1, 1, 1], 900, H)).toBe(1); // narrow window: no 300px columns
    expect(autoColumns(Array(20).fill(1), W, H)).toBeLessThanOrEqual(4); // never below the minimum width
  });

  it("lays out lanes automatically, keeping the card order", () => {
    const lanes = [["/a", "claude#1:/a", "/b"], ["/c"], [], []];
    expect(autoLanes(lanes, of, 2000, 1100)).toEqual([["/a", "claude#1:/a"], ["/b"], ["/c"]]);
    expect(autoLanes([[], []], of, 2000, 1100)).toEqual([[]]);
  });

  it("places a new project in an empty lane, then a new one while there is room", () => {
    expect(place([[]], "/a", of, 3)).toEqual([["/a"]]);
    expect(place([["/a"]], "/b", of, 3)).toEqual([["/a"], ["/b"]]);
    expect(place([["/a"], ["/b"], ["/c"]], "/d", of, 3)).toEqual([["/a", "/d"], ["/b"], ["/c"]]);
    expect(place([["/a", "/b"], ["/c"]], "/d", of, 2)).toEqual([["/a", "/b"], ["/c", "/d"]]);
  });

  it("places another session of a project in its card, wherever there is room", () => {
    const l = [["/a"], ["/b"]];
    expect(place(l, "claude#1:/b", of, 6)).toEqual([["/a"], ["/b", "claude#1:/b"]]);
    expect(place(l, "/a", of, 6)).toBe(l);
  });

  it("moves a project card to a new lane", () => {
    const l = [["/a", "claude#1:/a", "/b"], ["/c"]];
    expect(moveGroupToNewLane(l, "/a", of, 0)).toEqual([["/a", "claude#1:/a"], ["/b"], ["/c"]]);
    expect(moveGroupToNewLane(l, "/b", of, 2)).toEqual([["/a", "claude#1:/a"], ["/c"], ["/b"]]);
    expect(moveGroupToNewLane(l, "/c", of, 1)).toEqual([["/a", "claude#1:/a", "/b"], ["/c"], []]);
    expect(moveGroupToNewLane(l, "/x", of, 0)).toBe(l);
  });

  it("reorders sessions inside their card only", () => {
    const l = [["/b", "/a", "claude#1:/a", "shell#0:/a"], ["/c"]];
    expect(moveInGroup(l, "shell#0:/a", of, 0)).toEqual([["/b", "shell#0:/a", "/a", "claude#1:/a"], ["/c"]]);
    expect(moveInGroup(l, "/a", of, 2)).toEqual([["/b", "claude#1:/a", "shell#0:/a", "/a"], ["/c"]]);
    expect(moveInGroup(l, "/a", of, 99)).toEqual([["/b", "claude#1:/a", "shell#0:/a", "/a"], ["/c"]]);
    expect(moveInGroup(l, "/x", of, 0)).toBe(l);
  });

  it("drops empty lanes together with their widths", () => {
    expect(prune([["/a"], [], ["/b"]], [2, 1, 3])).toEqual({ lanes: [["/a"], ["/b"]], widths: [2, 3] });
    expect(prune([[], []], [1, 1])).toEqual({ lanes: [[]], widths: [1] });
    expect(prune([["/a"]], [])).toEqual({ lanes: [["/a"]], widths: [1] });
  });

  it("splits two neighbours' weight by where the line was dragged", () => {
    expect(split(2, 100, 100, 50, 10)).toEqual([1.5, 0.5]);
    expect(split(2, 100, 100, -500, 10)).toEqual([0.1, 1.9]); // neither side below the minimum
    expect(split(3, 300, 0, 0, 0)).toEqual([3, 0]);
  });
});
