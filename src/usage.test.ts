import { describe, expect, it } from "vitest";
import { level, resetsIn, usageHtml } from "./usage";

describe("usage chips", () => {
  it("colors by the worst window", () => {
    expect(level(42)).toBe("");
    expect(level(70)).toBe("warn");
    expect(level(95)).toBe("high");
    const html = usageHtml(
      [
        { agent: "claude", windows: [{ label: "5h", usedPercent: 42.4, resetsAt: 7200 }, { label: "7d", usedPercent: 91, resetsAt: 90000 }] },
        { agent: "codex", windows: [] },
      ],
      0,
    );
    expect(html).toContain("claude 5h 42% · 7d 91%");
    expect(html).toContain('class="usage high"');
    expect(html).not.toContain("codex");
  });

  it("formats time to reset", () => {
    expect(resetsIn(1800, 0)).toBe("30 min");
    expect(resetsIn(5400, 0)).toBe("1 h 30 min");
    expect(resetsIn(3 * 86400 + 7200, 0)).toBe("3 d 2 h");
  });
});
