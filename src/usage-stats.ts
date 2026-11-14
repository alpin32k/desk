// Pace, forecast and charts for the usage chip and its popover, from the
// plan limits (api.ts Usage) and desk's history of them (LimitSample).
// Times are Unix seconds unless named `...Ms`.
import type { LimitSample, Usage } from "./api";
import { language, t } from "./i18n";

export type Level = "" | "warn" | "high";

const RANK: Record<Level, number> = { "": 0, warn: 1, high: 2 };
const DAY = 86400;
const WEEK = 7 * DAY;

export function level(percent: number): Level {
  if (percent >= 90) return "high";
  if (percent >= 70) return "warn";
  return "";
}

function worse(a: Level, b: Level): Level {
  return RANK[b] > RANK[a] ? b : a;
}

/** A window's length from its label ("5h", "7d"); a model's weekly limit with `weekly`. */
export function windowSeconds(label: string, weekly = false): number | null {
  const m = /^(\d+)([hd])$/.exec(label);
  if (m) return Number(m[1]) * (m[2] === "h" ? 3600 : DAY);
  return weekly ? WEEK : null;
}

export interface Forecast {
  /** Percent per hour so far in this window. */
  pacePerHour: number;
  /** When it reaches 100% at this pace, if that comes before the reset. */
  endsAt: number | null;
  /** Seconds from reaching 100% to the reset; 0 when it lasts. */
  beforeReset: number;
  /** Where it ends at the reset, at this pace (may pass 100). */
  endPercent: number;
  /** Percent a day left until the reset, for windows of 2 days or more. */
  dailyBudget: number | null;
}

/** The pace so far carried on to the reset; null in the first 10% of the window or after the reset. */
export function forecast(usedPercent: number, resetsAt: number, lengthSec: number, nowMs: number): Forecast | null {
  const now = nowMs / 1000;
  const left = resetsAt - now;
  const elapsed = lengthSec - left;
  if (left <= 0 || elapsed < lengthSec * 0.1) return null;
  const pacePerHour = usedPercent / (elapsed / 3600);
  const toFull = pacePerHour > 0 ? (Math.max(0, 100 - usedPercent) / pacePerHour) * 3600 : Infinity;
  const endsAt = toFull < left ? now + toFull : null;
  return {
    pacePerHour,
    endsAt,
    beforeReset: endsAt === null ? 0 : resetsAt - endsAt,
    endPercent: usedPercent + pacePerHour * (left / 3600),
    dailyBudget: lengthSec >= 2 * DAY ? Math.max(0, 100 - usedPercent) / (left / DAY) : null,
  };
}

/** Anthropic's grading when there is one, else the 70/90% thresholds. */
export function severityLevel(severity: string | null, percent: number): Level {
  if (severity === "critical") return "high";
  if (severity === "warning") return "warn";
  if (severity === "normal") return "";
  return level(percent);
}

/** "dziś ok. 21:00" or "pt ok. 09:30", local time. */
export function around(at: number, nowMs: number): string {
  const d = new Date(at * 1000);
  const time = d.toLocaleTimeString(language(), { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === new Date(nowMs).toDateString()) return t("usage.endsToday", { time });
  return t("usage.endsOn", { day: d.toLocaleDateString(language(), { weekday: "short" }), time });
}

/**
 * The chip's color from the worst of every limit: its windows, its model
 * limits and the pace (100% before the reset is at least "warn"). The dot
 * shows when that comes from something the chip's text does not show (a
 * model limit or the pace); `reasons` name those things.
 */
export function chipState(u: Usage, nowMs: number): { level: Level; dot: boolean; reasons: string[] } {
  let shown: Level = "";
  let hidden: Level = "";
  const reasons: string[] = [];
  const pace = (label: string, used: number, resetsAt: number, length: number | null) => {
    const f = length === null ? null : forecast(used, resetsAt, length, nowMs);
    if (f?.endsAt == null) return;
    hidden = worse(hidden, "warn");
    reasons.push(t("usage.reasonForecast", { label, time: around(f.endsAt, nowMs) }));
  };
  for (const w of u.windows) {
    shown = worse(shown, severityLevel(w.severity, w.usedPercent));
    pace(w.label, w.usedPercent, w.resetsAt, windowSeconds(w.label));
  }
  for (const m of u.models) {
    const l = severityLevel(m.severity, m.usedPercent);
    if (l) {
      hidden = worse(hidden, l);
      const sev = { critical: t("usage.severityCritical"), warning: t("usage.severityWarning") }[m.severity ?? ""];
      const text = t("usage.reasonModel", { name: m.name, percent: Math.round(m.usedPercent) });
      reasons.unshift(sev ? `${text} (${sev})` : text);
    }
    pace(m.name, m.usedPercent, m.resetsAt, windowSeconds(m.name, true));
  }
  return { level: worse(shown, hidden), dot: RANK[hidden] > RANK[shown], reasons };
}

function fiveHour(history: LimitSample[]): LimitSample[] {
  return history.filter((h) => h.label === "5h").sort((a, b) => a.t - b.t);
}

/** A history bar's color: red only at the limit itself. */
export function barLevel(percent: number): Level {
  if (percent >= 100) return "high";
  if (percent >= 70) return "warn";
  return "";
}

/**
 * The last 24 hours of the 5h window in 48 half-hour bars, oldest first:
 * the highest reading in each, the last one carried on until its window
 * resets, 0 after that.
 */
export function bars24h(history: LimitSample[], nowMs: number): { percent: number; level: Level }[] {
  const samples = fiveHour(history);
  const now = nowMs / 1000;
  return Array.from({ length: 48 }, (_, i) => {
    const from = now - (48 - i) * 1800;
    const to = from + 1800;
    const before = samples.filter((x) => x.t < from).pop();
    let percent = before && before.resets > from ? before.pct : 0;
    for (const x of samples) if (x.t >= from && x.t < to) percent = Math.max(percent, x.pct);
    return { percent, level: barLevel(percent) };
  });
}

/** The 5h window's resets in the last 24 hours, `x` from 0 (24 h ago) to 1 (now). */
export function resetMarks(history: LimitSample[], nowMs: number): { at: number; x: number }[] {
  const now = nowMs / 1000;
  const resets = new Set(fiveHour(history).map((x) => x.resets));
  return [...resets]
    .filter((at) => at > now - DAY && at <= now)
    .sort((a, b) => a - b)
    .map((at) => ({ at, x: (at - (now - DAY)) / DAY }));
}

/** When the 5h window stood at 100%: from the reading that got there to its reset (or a lower reading). */
export function fullSpans(history: LimitSample[], nowMs: number): { from: number; to: number }[] {
  const now = nowMs / 1000;
  const spans: { from: number; to: number }[] = [];
  let open: LimitSample | null = null;
  for (const x of fiveHour(history)) {
    if (open && (x.pct < 100 || x.resets !== open.resets)) {
      spans.push({ from: open.t, to: Math.min(x.t, open.resets, now) });
      open = null;
    }
    if (!open && x.pct >= 100) open = x;
  }
  if (open) spans.push({ from: open.t, to: Math.min(open.resets, now) });
  return spans;
}

/**
 * The week up to `resetsAt` in local days: how much `label` grew in each
 * (its highest reading in the day less the highest before it).
 */
export function weekDays(history: LimitSample[], label: string, resetsAt: number): { dayStart: number; delta: number }[] {
  const start = resetsAt - WEEK;
  const samples = history.filter((h) => h.label === label && h.t >= start && h.t < resetsAt);
  const first = new Date(start * 1000);
  const days: { dayStart: number; delta: number }[] = [];
  let top = 0;
  for (let i = 0; ; i++) {
    const dayStart = new Date(first.getFullYear(), first.getMonth(), first.getDate() + i).getTime() / 1000;
    if (dayStart >= resetsAt) break;
    const next = new Date(first.getFullYear(), first.getMonth(), first.getDate() + i + 1).getTime() / 1000;
    const dayTop = Math.max(top, ...samples.filter((x) => x.t >= dayStart && x.t < next).map((x) => x.pct));
    days.push({ dayStart, delta: dayTop - top });
    top = dayTop;
  }
  return days;
}
