import { describe, expect, it } from "vitest";
import type { Usage, UsageWindow } from "./api";
import { chipId, level, resetsIn, shortReset, usageHtml } from "./usage";

/** A reading with only windows, as the status line gives it. */
function plain(agent: string, account: string | null, windows: Omit<UsageWindow, "severity">[]): Usage {
  return { agent, account, windows: windows.map((w) => ({ ...w, severity: null })), models: [], credits: null, resets: null };
}

describe("usage chips", () => {
  it("colors by the worst window", () => {
    expect(level(42)).toBe("");
    expect(level(70)).toBe("warn");
    expect(level(95)).toBe("high");
    const html = usageHtml(
      [
        plain("claude", null, [
          { label: "5h", usedPercent: 42.4, resetsAt: 7200 },
          { label: "7d", usedPercent: 91, resetsAt: 90000 },
        ]),
        plain("codex", null, []),
      ],
      0,
    );
    expect(html).toContain('claude 5h 42% <span class="reset">↻2h00m</span> · 7d 91% <span class="reset">↻1d1h</span>');
    expect(html).toContain('<button class="usage high" data-usage="claude:" aria-haspopup="dialog"');
    expect(html).not.toContain('<i class="dot">');
    expect(html).not.toContain("codex");
  });

  it("is a button per agent and account", () => {
    expect(chipId(plain("claude", "work", []))).toBe("claude:work");
    expect(chipId(plain("codex", null, []))).toBe("codex:");
  });

  it("turns red with a dot when a model limit is critical", () => {
    const s = 1_000_000;
    const u: Usage = {
      ...plain("claude", null, [
        { label: "5h", usedPercent: 61, resetsAt: s + 6840 },
        { label: "7d", usedPercent: 54, resetsAt: s + 6 * 86400 },
      ]),
      models: [{ name: "Fable", usedPercent: 95, resetsAt: s + 6 * 86400, severity: "critical", shareOfWeekly: 48, allowanceOfWeekly: 50 }],
    };
    const html = usageHtml([u], s * 1000);
    expect(html).toContain('class="usage high"');
    expect(html).toContain('<i class="dot"></i></button>');
    expect(html).toContain("Fable 95%");
    expect(html).toContain('claude 5h 61% <span class="reset">↻1h54m</span> · 7d 54%');
  });

  it("names each Claude account by its own name and color", () => {
    const names: Record<string, { name: string; color: string }> = {
      work: { name: "Firma", color: "#e3b341" },
      own: { name: "<Prywatne>", color: "#56d4dd" },
    };
    const html = usageHtml(
      [
        plain("claude", "work", [{ label: "5h", usedPercent: 94, resetsAt: 7200 }]),
        plain("claude", "own", [{ label: "5h", usedPercent: 12, resetsAt: 7200 }]),
      ],
      0,
      (id) => names[id] ?? null,
    );
    expect(html).toContain('<i class="acct" style="background:#e3b341"></i><b>Firma</b> 5h 94%');
    expect(html).toContain("<b>&#60;Prywatne&#62;</b> 5h 12%");
    expect(html).not.toContain("claude 5h");
  });

  it("formats time to reset", () => {
    expect(resetsIn(1800, 0)).toBe("30 min");
    expect(resetsIn(5400, 0)).toBe("1 h 30 min");
    expect(resetsIn(3 * 86400 + 7200, 0)).toBe("3 d 2 h");
    expect(shortReset(1800, 0)).toBe("30m");
    expect(shortReset(5400 + 180, 0)).toBe("1h33m");
    expect(shortReset(3 * 86400 + 7200, 0)).toBe("3d2h");
  });
});
