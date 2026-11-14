import { describe, expect, it } from "vitest";
import type { Layout, Usage } from "./api";
import { leftBy, shareRequest, stillOpen } from "./usage-popover";

function usage(account: string | null): Usage {
  return {
    agent: "claude",
    account,
    windows: [{ label: "5h", usedPercent: 10, resetsAt: 9000, severity: null }],
    models: [],
    credits: null,
    resets: null,
  };
}

function layout(usageLayout: Layout["usageLayout"], usageShare: Layout["usageShare"]): Layout {
  return { usageLayout, usageShare } as Layout;
}

describe("the usage popover", () => {
  it("closes when its chip is gone", () => {
    expect(stillOpen("claude:work", [usage("own")])).toBeNull();
    expect(stillOpen("claude:work", [usage("own"), usage("work")])).toBe("claude:work");
    expect(stillOpen("claude:work", [{ ...usage("work"), windows: [] }])).toBeNull();
    expect(stillOpen(null, [usage("work")])).toBeNull();
  });

  it("asks for the share only where it is shown", () => {
    expect(shareRequest(layout("tabs", "cost"), "now")).toBeNull();
    expect(shareRequest(layout("tabs", "cost"), "history")).toBeNull();
    expect(shareRequest(layout("tabs", "cost"), "sessions")).toBe("cost");
    expect(shareRequest(layout("all", "cost"), "history")).toBe("cost");
    expect(shareRequest(layout("tabs", "transcripts"), "sessions")).toBe("transcripts");
  });

  it("is left by a click or focus anywhere but itself and the chips", () => {
    const at = (chip: boolean) => ({ closest: (sel: string) => (sel === "[data-usage]" && chip ? {} : null) });
    // Options or the palette opened from the keyboard take the focus.
    expect(leftBy(at(false), false)).toBe(true);
    expect(leftBy(at(false), true)).toBe(false);
    // The chip toggles it itself.
    expect(leftBy(at(true), false)).toBe(false);
    expect(leftBy(null, false)).toBe(false);
  });
});
