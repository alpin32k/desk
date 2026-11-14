import { describe, expect, it } from "vitest";
import type { LimitSample, Usage } from "./api";
import {
  bars24h,
  chipState,
  forecast,
  fullSpans,
  resetMarks,
  severityLevel,
  weekDays,
  windowSeconds,
} from "./usage-stats";

const now = Date.UTC(2026, 10, 14, 0, 55);
const s = now / 1000;
const H = 3600;

function sample(t: number, pct: number, resets: number, label = "5h"): LimitSample {
  return { t, agent: "claude", account: null, label, pct, resets };
}

describe("forecast", () => {
  it("forecasts the 5h window running out before its reset", () => {
    const f = forecast(50, s + 2 * H + 55 * 60, 18000, now)!;
    expect(f.pacePerHour).toBeCloseTo(24, 1);
    expect(f.beforeReset).toBeCloseTo(50 * 60, -1);
    expect(f.dailyBudget).toBeNull();
  });

  it("gives a daily budget for the week", () => {
    const f = forecast(52, s + (6 * 24 + 3) * H + 5 * 60, 604800, now)!;
    expect(f.dailyBudget).toBeCloseTo(7.83, 1);
    expect(f.endsAt! - s).toBeCloseTo(19.3 * H, -3);
  });

  it("has no end when the pace lasts past the reset", () => {
    const f = forecast(10, s + 2 * H, 18000, now)!;
    expect(f.endsAt).toBeNull();
    expect(f.beforeReset).toBe(0);
    expect(f.endPercent).toBeCloseTo(10 + (10 / 3) * 2, 5);
  });

  it("has no forecast too early or after the reset", () => {
    expect(forecast(5, s + 4 * H + 40 * 60, 18000, now)).toBeNull();
    expect(forecast(50, s - 1, 18000, now)).toBeNull();
  });
});

describe("limit levels", () => {
  it("knows each window's length", () => {
    expect(windowSeconds("5h")).toBe(18000);
    expect(windowSeconds("7d")).toBe(604800);
    expect(windowSeconds("Fable")).toBeNull();
    expect(windowSeconds("Fable", true)).toBe(604800);
  });

  it("takes Anthropic's grading over the thresholds", () => {
    expect(severityLevel("critical", 10)).toBe("high");
    expect(severityLevel("warning", 10)).toBe("warn");
    expect(severityLevel("normal", 95)).toBe("");
    expect(severityLevel(null, 95)).toBe("high");
  });

  it("turns the chip red with a dot for Fable at 95% critical", () => {
    const u: Usage = {
      agent: "claude",
      account: null,
      credits: false,
      resets: null,
      windows: [
        { label: "5h", usedPercent: 61, resetsAt: s + 6840, severity: "normal" },
        { label: "7d", usedPercent: 54, resetsAt: s + 5 * 86400, severity: "normal" },
      ],
      models: [
        { name: "Fable", usedPercent: 95, resetsAt: s + 5 * 86400, severity: "critical", shareOfWeekly: 48, allowanceOfWeekly: 50 },
      ],
    };
    const c = chipState(u, now);
    expect([c.level, c.dot]).toEqual(["high", true]);
    expect(c.reasons.join(" ")).toContain("Fable 95%");
  });

  it("keeps today's chip without model limits or warnings", () => {
    const u: Usage = {
      agent: "claude",
      account: null,
      credits: null,
      resets: null,
      models: [],
      windows: [{ label: "5h", usedPercent: 20, resetsAt: s + 3600, severity: null }],
    };
    expect(chipState(u, now)).toMatchObject({ level: "", dot: false, reasons: [] });
  });

  it("warns with a dot when the pace runs out before the reset", () => {
    const u: Usage = {
      agent: "claude",
      account: null,
      credits: null,
      resets: null,
      models: [],
      windows: [{ label: "5h", usedPercent: 50, resetsAt: s + 2 * H + 55 * 60, severity: null }],
    };
    const c = chipState(u, now);
    expect([c.level, c.dot]).toEqual(["warn", true]);
    expect(c.reasons).toHaveLength(1);
  });

  it("needs no dot when the text already shows the worst", () => {
    const u: Usage = {
      agent: "claude",
      account: null,
      credits: null,
      resets: null,
      models: [{ name: "Fable", usedPercent: 75, resetsAt: s + 5 * 86400, severity: null, shareOfWeekly: null, allowanceOfWeekly: null }],
      windows: [{ label: "5h", usedPercent: 92, resetsAt: s + 4 * H, severity: null }],
    };
    expect(chipState(u, now)).toMatchObject({ level: "high", dot: false });
  });
});

describe("history charts", () => {
  it("draws 48 half-hour bars of the 5h window", () => {
    const history = [sample(s - 10 * H, 40, s - 8 * H), sample(s - 9 * H, 95, s - 8 * H), sample(s - 2 * H, 10, s + 3 * H)];
    const bars = bars24h(history, now);
    expect(bars).toHaveLength(48);
    const pct = bars.map((b) => b.percent);
    expect(pct.slice(0, 28).every((p) => p === 0)).toBe(true);
    expect(pct.slice(28, 32)).toEqual([40, 40, 95, 95]);
    expect(pct.slice(32, 44).every((p) => p === 0)).toBe(true);
    expect(pct.slice(44)).toEqual([10, 10, 10, 10]);
    expect(bars[30].level).toBe("warn");
    expect(bars[28].level).toBe("");
    expect(bars24h([sample(s - H, 100, s + H)], now)[47].level).toBe("high");
  });

  it("marks the resets of the last 24 hours", () => {
    const marks = resetMarks([sample(s - 14 * H, 30, s - 12 * H), sample(s - 30 * H, 30, s - 28 * H)], now);
    expect(marks).toHaveLength(1);
    expect(marks[0].at).toBe(s - 12 * H);
    expect(marks[0].x).toBeCloseTo(0.5, 5);
  });

  it("takes reset times a second apart for one reset", () => {
    // Anthropic's answer and the status line give the same reset a second apart.
    const history = [sample(s - 14 * H, 30, s - 12 * H), sample(s - 13.5 * H, 100, s - 12 * H + 1), sample(s - 13 * H, 100, s - 12 * H)];
    expect(resetMarks(history, now)).toHaveLength(1);
    expect(fullSpans(history, now)).toEqual([{ from: s - 13.5 * H, to: s - 12 * H + 1 }]);
  });

  it("spans the time spent at the limit", () => {
    const spans = fullSpans([sample(s - 4 * H, 80, s - H), sample(s - 3 * H, 100, s - H), sample(s - 2 * H, 100, s - H)], now);
    expect(spans).toEqual([{ from: s - 3 * H, to: s - H }]);
  });

  it("splits the week into days by what each one used", () => {
    const start = new Date(2026, 10, 8).getTime() / 1000;
    const resetsAt = new Date(2026, 10, 15).getTime() / 1000;
    const history = [sample(start + 10 * H, 47, resetsAt, "7d"), sample(start + 34 * H, 52, resetsAt, "7d")];
    const days = weekDays(history, "7d", resetsAt);
    expect(days.map((d) => d.delta)).toEqual([47, 5, 0, 0, 0, 0, 0]);
    expect(days[0].dayStart).toBe(start);
  });

  it("has no data for the days before desk recorded, and counts from the first reading", () => {
    const start = new Date(2026, 10, 8).getTime() / 1000;
    const resetsAt = new Date(2026, 10, 15).getTime() / 1000;
    const since = start + 3 * 86400 + 10 * H;
    const history = [sample(since, 54, resetsAt, "7d"), sample(since + 6 * H, 60, resetsAt, "7d"), sample(since + 30 * H, 65, resetsAt, "7d")];
    const days = weekDays(history, "7d", resetsAt, since);
    expect(days.map((d) => d.delta)).toEqual([null, null, null, 6, 5, 0, 0]);
  });
});
