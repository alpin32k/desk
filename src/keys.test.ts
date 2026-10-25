import { describe, expect, it } from "vitest";
import { actionFor } from "./keys";

const k = (key: string, mods: Partial<{ metaKey: boolean; shiftKey: boolean; altKey: boolean; ctrlKey: boolean }> = {}) => ({
  key,
  metaKey: true,
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  ...mods,
});

describe("actionFor", () => {
  it("maps the documented shortcuts", () => {
    expect(actionFor(k("k"))).toEqual({ type: "palette" });
    expect(actionFor(k("n"))).toEqual({ type: "new" });
    expect(actionFor(k("w"))).toEqual({ type: "close" });
    expect(actionFor(k("t"))).toEqual({ type: "sub" });
    expect(actionFor(k("W", { shiftKey: true }))).toEqual({ type: "kill" });
    expect(actionFor(k("3"))).toEqual({ type: "focus", index: 2 });
    expect(actionFor(k("Enter"))).toEqual({ type: "fullscreen" });
    expect(actionFor(k("Escape"))).toEqual({ type: "exitFullscreen" });
    expect(actionFor(k(","))).toEqual({ type: "settings" });
  });

  it("leaves terminal keys alone", () => {
    expect(actionFor(k("Escape", { metaKey: false }))).toBeNull();
    expect(actionFor(k("c"))).toBeNull(); // copy
    expect(actionFor(k("v"))).toBeNull(); // paste
    expect(actionFor(k("k", { ctrlKey: true }))).toBeNull();
    expect(actionFor(k("0"))).toBeNull();
  });
});
