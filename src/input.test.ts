import { describe, expect, it } from "vitest";
import { stripProbeReplies } from "./input";

describe("stripProbeReplies", () => {
  it("drops Device Attributes replies and keeps typed keys", () => {
    expect(stripProbeReplies("\x1b[?1;2c")).toBe("");
    expect(stripProbeReplies("\x1b[>0;276;0c")).toBe("");
    expect(stripProbeReplies("a\x1b[?1;2cb")).toBe("ab");
    expect(stripProbeReplies("c")).toBe("c");
    expect(stripProbeReplies("\x1b[A")).toBe("\x1b[A"); // arrow up
    expect(stripProbeReplies("\x1b[12;40R")).toBe("\x1b[12;40R"); // cursor position report
    expect(stripProbeReplies("?1;2c")).toBe("?1;2c"); // typed text stays
  });
});
