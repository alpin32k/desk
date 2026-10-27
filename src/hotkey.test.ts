import { describe, expect, it } from "vitest";
import { acceleratorFor, DEFAULT_HOTKEY, hotkeyLabel } from "./hotkey";

const key = (code: string, mods: Partial<{ metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean }> = {}) => ({
  key: "",
  code,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("acceleratorFor", () => {
  it("names the physical key with its modifiers", () => {
    expect(acceleratorFor(key("Backquote", { metaKey: true }))).toBe("Super+Backquote");
    expect(acceleratorFor(key("KeyD", { altKey: true, shiftKey: true }))).toBe("Alt+Shift+KeyD");
    expect(acceleratorFor(key("Space", { ctrlKey: true }))).toBe("Control+Space");
  });

  it("waits for a key after the modifiers", () => {
    expect(acceleratorFor(key("MetaLeft", { metaKey: true }))).toBeNull();
    expect(acceleratorFor(key("ShiftRight", { shiftKey: true }))).toBeNull();
  });

  it("takes a plain key only when it is an F-key", () => {
    expect(acceleratorFor(key("KeyA"))).toBeNull();
    expect(acceleratorFor(key("KeyA", { shiftKey: true }))).toBeNull(); // typing A
    expect(acceleratorFor(key("F12"))).toBe("F12");
  });
});

describe("hotkeyLabel", () => {
  it("writes macOS symbols in their usual order", () => {
    expect(hotkeyLabel(DEFAULT_HOTKEY("mac"), "mac")).toBe("⌘`");
    expect(hotkeyLabel("Super+Shift+Control+KeyD", "mac")).toBe("⌃⇧⌘D");
    expect(hotkeyLabel("Alt+Space", "mac")).toBe("⌥Space");
  });

  it("writes it like desk's other shortcuts on Windows and Linux", () => {
    expect(hotkeyLabel(DEFAULT_HOTKEY("windows"), "windows")).toBe("Ctrl+`");
    expect(hotkeyLabel("Control+Shift+Digit1", "linux")).toBe("Ctrl⇧1");
    expect(hotkeyLabel("Control+Shift+Backspace", "windows")).toBe("Ctrl⇧⌫");
    expect(hotkeyLabel("Control+Alt+KeyD", "linux")).toBe("Ctrl+Alt+D");
  });
});
