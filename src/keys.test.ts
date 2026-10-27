import { describe, expect, it } from "vitest";
import { actionFor, hints } from "./keys";

const k = (key: string, mods: Partial<{ metaKey: boolean; shiftKey: boolean; altKey: boolean; ctrlKey: boolean }> = {}) => ({
  key,
  metaKey: true,
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  ...mods,
});

const mac = (e: Parameters<typeof actionFor>[0]) => actionFor(e, "mac");

describe("actionFor", () => {
  it("maps the Polish-named shortcuts", () => {
    expect(mac(k("p"))).toEqual({ type: "palette" });
    expect(mac(k("n"))).toEqual({ type: "new" });
    expect(mac(k("d"))).toEqual({ type: "sub" });
    expect(mac(k("q"))).toEqual({ type: "close" });
    expect(mac(k("z"))).toBeNull(); // ⌘Z is undo again
    expect(mac(k("Q", { shiftKey: true }))).toEqual({ type: "kill" });
    expect(mac(k("Z", { shiftKey: true }))).toBeNull(); // redo
    expect(mac(k("3"))).toEqual({ type: "focus", index: 2 });
    expect(mac(k("e"))).toEqual({ type: "fullscreen" });
    expect(mac(k("Enter"))).toEqual({ type: "fullscreen" });
    expect(mac(k("j"))).toEqual({ type: "minimize" });
    expect(mac(k("Escape"))).toEqual({ type: "exitFullscreen" });
    expect(mac(k("k"))).toEqual({ type: "coffee" });
    expect(mac(k("b"))).toEqual({ type: "lock" });
    expect(mac(k("r"))).toEqual({ type: "chat" });
    expect(mac(k("t"))).toEqual({ type: "terminal" });
    expect(mac(k("f"))).toEqual({ type: "files" });
    expect(mac(k("u"))).toEqual({ type: "run" });
    expect(mac(k("o"))).toEqual({ type: "settings" });
    expect(mac(k("w"))).toEqual({ type: "preview" });
    expect(mac(k("="))).toEqual({ type: "font", step: 1 });
    expect(mac(k("-"))).toEqual({ type: "font", step: -1 });
    expect(mac(k("0"))).toEqual({ type: "font", step: 0 });
  });

  it("leaves terminal keys alone", () => {
    expect(mac(k("Escape", { metaKey: false }))).toBeNull();
    expect(mac(k("c"))).toBeNull(); // copy
    expect(mac(k("v"))).toBeNull(); // paste
    expect(mac(k("p", { ctrlKey: true }))).toBeNull();
    expect(mac(k("0", { altKey: true }))).toBeNull();
    expect(mac(k("W", { shiftKey: true }))).toBeNull();
  });

  it("uses Ctrl+Shift on Windows and Linux", () => {
    const w = (code: string, key = "", mods: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean }> = {}) =>
      actionFor({ key, code, metaKey: false, ctrlKey: true, shiftKey: true, altKey: false, ...mods }, "windows");
    expect(w("KeyP", "P")).toEqual({ type: "palette" });
    expect(w("Digit3", "#")).toEqual({ type: "focus", index: 2 }); // Shift turns 3 into #
    expect(w("Backspace", "Backspace")).toEqual({ type: "kill" });
    expect(w("KeyQ", "Q")).toEqual({ type: "close" });
    expect(w("KeyZ", "Z")).toBeNull();
    expect(w("KeyC", "C")).toBeNull(); // copy, handled by the terminal tile
    expect(w("Equal", "+")).toEqual({ type: "font", step: 1 });
    expect(w("Minus", "_")).toEqual({ type: "font", step: -1 });
    expect(w("Escape", "Escape")).toBeNull(); // Task Manager
    expect(w("KeyP", "p", { shiftKey: false })).toBeNull(); // Ctrl+P is the shell's
    expect(w("KeyZ", "Ż", { altKey: true })).toBeNull(); // AltGr+Shift+Z types Ż
    expect(actionFor(k("p"), "windows")).toBeNull(); // the Windows key is not ⌘
  });

  it("shows shortcuts for the platform", () => {
    expect(hints("mac")[0]).toEqual(["⌘P", "projekty"]);
    expect(hints("windows")[0]).toEqual(["Ctrl⇧P", "projekty"]);
    expect(hints("linux").find(([, l]) => l === "zabij")).toEqual(["Ctrl⇧⌫", "zabij"]);
  });
});
