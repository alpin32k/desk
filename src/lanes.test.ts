import { describe, expect, it } from "vitest";
import { add, flatten, move, remove, restore, setLaneCount, shortestLane } from "./lanes";

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
});
