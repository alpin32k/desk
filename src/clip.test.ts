import { describe, expect, it } from "vitest";
import { MAX_CLIP, mayWrite } from "./clip";

describe("clipboard writes from programs", () => {
  it("are allowed only right after your own input", () => {
    expect(mayWrite("x", 1000, 2500)).toBe(true);
    expect(mayWrite("x", 1000, 4001)).toBe(false); // printed later, nobody asked
    expect(mayWrite("", 1000, 1000)).toBe(false);
    expect(mayWrite("a".repeat(MAX_CLIP + 1), 1000, 1000)).toBe(false);
  });
});
