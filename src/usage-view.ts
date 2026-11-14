// The usage popover's content as HTML: "Now" (limits, pace, model limits,
// resets), "Sessions" (each Claude session's share of the 5-hour window)
// and "History" (the last 24 hours and the week). usage-popover.ts shows it.
import type { ModelLimit, SessionStatus, Share, Usage, UsageDetails, UsageWindow } from "./api";
import { language, t } from "./i18n";
import { resetsIn } from "./usage";
import {
  around,
  bars24h,
  chipState,
  forecast,
  fullSpans,
  resetMarks,
  severityLevel,
  weekDays,
  windowSeconds,
  type Level,
} from "./usage-stats";

/** A desk session in the sessions table: its tile's name and hook state, and its status line. */
export interface SessionView {
  slug: string;
  /** The tile's key, to focus it. */
  key: string;
  name: string;
  state: "working" | "waiting" | "idle";
  status: SessionStatus;
}

const DAY = 86400;
const WARN_ICON =
  '<svg class="up-icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2 15 14H1Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M8 6.5v3.2M8 11.6v.3" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>';

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

function cls(base: string, level: string): string {
  return level ? `${base} ${level}` : base;
}

function clock(at: number): string {
  return new Date(at * 1000).toLocaleTimeString(language(), { hour: "2-digit", minute: "2-digit" });
}

/** "21:49" today, else "pt 10:00". */
function dayClock(at: number, nowMs: number): string {
  const d = new Date(at * 1000);
  if (d.toDateString() === new Date(nowMs).toDateString()) return clock(at);
  return `${d.toLocaleDateString(language(), { weekday: "short" })} ${clock(at)}`;
}

function date(at: number): string {
  return new Date(at * 1000).toLocaleDateString(language(), { day: "2-digit", month: "2-digit" });
}

function pct(n: number): string {
  return `${Math.round(n)}%`;
}

function pace(n: number): string {
  return n >= 10 ? String(Math.round(n)) : n.toFixed(1);
}

function tokens(n: number): string {
  return n >= 1000 ? `${Math.round(n / 1000)}k` : String(n);
}

function warn(level: Level, text: string): string {
  return `<p class="${cls("up-warn", level || "warn")}">${WARN_ICON}<span>${text}</span></p>`;
}

function line(text: string): string {
  return `<p class="up-line">${text}</p>`;
}

/** A limit's bar with the share of its window gone as a tick, and the percent. */
function meter(used: number, level: Level, gone: number | null, model = false): string {
  const tick = gone === null ? "" : `<b style="left:${(gone * 100).toFixed(1)}%"></b>`;
  const elapsed =
    gone === null
      ? ""
      : `<p class="up-elapsed" style="--x:${(gone * 100).toFixed(1)}%"><span>${t("usage.elapsed", { n: Math.round(gone * 100) })}</span></p>`;
  return `<div class="up-meter"><div class="${cls(model ? "up-bar model" : "up-bar", level)}"><i style="width:${Math.min(100, used).toFixed(1)}%"></i>${tick}</div><span class="${cls("up-pct", level)}">${pct(used)}</span></div>${elapsed}`;
}

/** Reset time, pace and forecast of one limit. */
function limitLines(used: number, resetsAt: number, length: number | null, nowMs: number): string {
  let out = line(t("usage.resetAt", { when: dayClock(resetsAt, nowMs), left: resetsIn(resetsAt, nowMs) }));
  const f = length === null ? null : forecast(used, resetsAt, length, nowMs);
  if (!f) return out;
  if (f.endsAt !== null) {
    const left = resetsIn(nowMs / 1000 + f.beforeReset, nowMs);
    out += warn(f.beforeReset > DAY ? "high" : "warn", t("usage.paceOut", { pace: pace(f.pacePerHour), when: around(f.endsAt, nowMs), left }));
  } else {
    out += line(t("usage.paceOk", { pace: pace(f.pacePerHour), end: Math.round(f.endPercent) }));
  }
  if (f.dailyBudget !== null) out += line(t("usage.dailyBudget", { n: f.dailyBudget.toFixed(1) }));
  return out;
}

function gone(resetsAt: number, length: number | null, nowMs: number): number | null {
  if (length === null) return null;
  return Math.min(1, Math.max(0, (nowMs / 1000 - (resetsAt - length)) / length));
}

function windowName(label: string): string {
  if (label === "5h") return t("usage.fiveHour");
  if (label === "7d") return t("usage.week");
  return escapeHtml(label);
}

function windowHtml(w: UsageWindow, nowMs: number): string {
  const length = windowSeconds(w.label);
  const level = severityLevel(w.severity, w.usedPercent);
  return `<section class="up-sec"><h4>${windowName(w.label)}</h4>${meter(w.usedPercent, level, gone(w.resetsAt, length, nowMs))}${limitLines(w.usedPercent, w.resetsAt, length, nowMs)}</section>`;
}

function modelHtml(m: ModelLimit, credits: boolean | null, nowMs: number): string {
  const length = windowSeconds(m.name, true);
  const level = severityLevel(m.severity, m.usedPercent);
  const name = escapeHtml(m.name);
  let out = `<div class="up-mlimit"><p class="up-model">${name}</p>${meter(m.usedPercent, level, gone(m.resetsAt, length, nowMs), true)}`;
  if (level) {
    const sev = { critical: t("usage.severityCritical"), warning: t("usage.severityWarning") }[m.severity ?? ""];
    const text = t("usage.reasonModel", { name, percent: Math.round(m.usedPercent) });
    out += warn(level, sev ? `${text} (${sev})` : text);
  }
  if (m.shareOfWeekly !== null && m.allowanceOfWeekly !== null) {
    out += line(t("usage.modelShare", { share: Math.round(m.shareOfWeekly), allowance: Math.round(m.allowanceOfWeekly) }));
  }
  out += limitLines(m.usedPercent, m.resetsAt, length, nowMs);
  if (credits !== null) out += line(t(credits ? "usage.credits" : "usage.noCredits"));
  return `${out}</div>`;
}

function resetsHtml(u: Usage, nowMs: number): string {
  const r = u.resets;
  if (!r || (!r.fiveHour && !r.saved.length)) return "";
  let out = "";
  if (r.fiveHour) {
    const f = r.fiveHour;
    if (f.available) out += line(t("usage.fiveHourReset"));
    else if (f.nextAvailableAt !== null) out += line(t("usage.fiveHourResetAgain", { when: dayClock(f.nextAvailableAt, nowMs) }));
    else out += line(t("usage.fiveHourResetUsed"));
    const five = u.windows.find((w) => w.label === "5h");
    const end = five ? forecast(five.usedPercent, five.resetsAt, 18000, nowMs) : null;
    if (f.available && end?.endsAt != null) out += warn("warn", t("usage.resetHint", { time: clock(end.endsAt) }));
  }
  for (const s of r.saved) {
    const parts = [t("usage.saved", { label: escapeHtml(s.label), left: s.left, total: s.total })];
    if (s.endsAt !== null) parts.push(t("usage.savedUntil", { date: date(s.endsAt) }));
    if (s.requiresLimit) parts.push(t("usage.savedAtLimit"));
    out += line(parts.join(" · "));
  }
  out += line(`<span class="up-dim">${t("usage.resetHow")}</span>`);
  return `<section class="up-sec up-opt" data-section="resets"><h4>${t("usage.resets")}</h4>${out}</section>`;
}

/** Windows, then model limits and resets when there are any. */
export function renderNow(u: Usage, nowMs: number): string {
  const models = u.models.length
    ? `<section class="up-sec up-opt" data-section="models"><h4>${t("usage.models")}</h4>${u.models.map((m) => modelHtml(m, u.credits, nowMs)).join("")}</section>`
    : "";
  return u.windows.map((w) => windowHtml(w, nowMs)).join("") + models + resetsHtml(u, nowMs);
}

function cacheCell(s: SessionStatus, nowMs: number): string {
  if (s.cacheWarm === false) return `<span class="up-cold">${t("usage.cacheCold")}</span>`;
  if (s.cacheWarm && s.cacheExpiresAt !== null) return t("usage.cacheWarm", { left: resetsIn(s.cacheExpiresAt, nowMs) });
  return "—";
}

function shareCell(fraction: number | null, model: boolean): string {
  if (fraction === null) return "—";
  return `<span class="${model ? "up-mini model" : "up-mini"}"><i style="width:${(fraction * 100).toFixed(1)}%"></i></span>${pct(fraction * 100)}`;
}

/**
 * The account's desk sessions, by share of the 5-hour window and then cost,
 * a row for the share of sessions not among them, and the largest cold
 * cache. `models` names the account's model limits (shown in their color).
 */
export function renderSessions(
  rows: SessionView[],
  share: Share | null | "loading",
  wide: boolean,
  nowMs: number,
  models: string[] = [],
): string {
  const got = share && share !== "loading" ? share : null;
  const fraction = (slug: string) => (got ? (got.rows.find((r) => r.slug === slug)?.fraction ?? 0) : null);
  const sorted = [...rows].sort((a, b) => (fraction(b.slug) ?? 0) - (fraction(a.slug) ?? 0) || b.status.cost - a.status.cost);
  const open = new Set(rows.map((r) => r.slug));
  const rest = got ? got.rows.filter((r) => r.slug === null || !open.has(r.slug)) : [];
  const isModel = (s: SessionStatus) => models.some((m) => s.model.toLowerCase().startsWith(m.toLowerCase()));
  const shareHead = `${t("usage.colShare")} <span class="up-dim">${t("usage.estimate")}${got?.incomplete ? ` · ${t("usage.incomplete")}` : ""}</span>`;
  const head = `<tr><th>${t("usage.colSession")}</th><th>${t("usage.colModel")}</th><th>${shareHead}</th><th>${t("usage.colCost")}</th><th>${t("usage.colCache")}</th>${wide ? `<th>${t("usage.colContext")}</th>` : ""}</tr>`;
  const body = sorted
    .map((r) => {
      const s = r.status;
      const model = `${escapeHtml(s.model)}${s.effort ? ` · ${escapeHtml(s.effort)}` : ""}${s.fast ? " ⚡" : ""}`;
      const context = wide ? `<td>${s.contextPercent === null ? "—" : pct(s.contextPercent)}</td>` : "";
      return `<tr class="up-srow" data-key="${escapeHtml(r.key)}"><td><i class="up-st ${r.state}"></i>${escapeHtml(r.name)}<span class="up-go">${t("usage.toTile")}</span></td><td${isModel(s) ? ' class="up-model"' : ""}>${model}</td><td>${shareCell(fraction(r.slug), isModel(s))}</td><td>$${s.cost.toFixed(2)}</td><td>${cacheCell(s, nowMs)}</td>${context}</tr>`;
    })
    .join("");
  let restRow = "";
  if (rest.length) {
    const names = got?.mode === "transcripts" ? rest.map((r) => r.label) : [t("usage.restClosed")];
    const label = `${t("usage.rest")} <span class="up-dim">(${escapeHtml(names.slice(0, 3).join(", "))}${names.length > 3 ? ", …" : ""})</span>`;
    const sum = rest.reduce((a, r) => a + r.fraction, 0);
    restRow = `<tr class="up-rest"><td>${label}</td><td></td><td>${shareCell(sum, false)}</td><td></td><td></td>${wide ? "<td></td>" : ""}</tr>`;
  }
  if (!sorted.length && !restRow) return `<p class="up-line up-dim">${t("usage.noSessions")}</p>`;
  const cold = rows
    .filter((r) => r.status.cacheWarm === false && (r.status.recacheTokens ?? 0) > 50_000)
    .sort((a, b) => (b.status.recacheTokens ?? 0) - (a.status.recacheTokens ?? 0))[0];
  const coldLine = cold
    ? warn("warn", t("usage.coldWarning", { name: escapeHtml(cold.name), tokens: tokens(cold.status.recacheTokens ?? 0) }))
    : "";
  const loading = share === "loading" ? `<p class="up-line up-dim">${t("usage.shareLoading")}</p>` : "";
  const total = rows.reduce((a, r) => a + r.status.cost, 0);
  const how = got ? t(got.mode === "transcripts" ? "settings.usageShareTranscripts" : "settings.usageShareCost") : "";
  const mode = got ? ` · ${t("usage.shareBy", { mode: how.charAt(0).toLowerCase() + how.slice(1) })}` : "";
  return `${loading}<table class="up-table"><thead>${head}</thead><tbody>${body}${restRow}</tbody></table>${coldLine}<p class="up-line up-dim">${t("usage.sessionsTotal", { cost: `$${total.toFixed(2)}` })}${mode}</p>`;
}

/** The last 24 hours of the 5-hour window, and the week day by day. */
export function renderHistory(d: UsageDetails, u: Usage, nowMs: number): string {
  const now = nowMs / 1000;
  const note =
    d.recordingSince === null || d.recordingSince > now - DAY
      ? `<p class="up-line up-dim">${t("usage.collecting", { time: clock(d.recordingSince ?? now) })}</p>`
      : "";
  const bars = bars24h(d.history, nowMs)
    .map((b) => `<i class="${cls("up-b", b.level)}" style="height:${Math.min(100, b.percent).toFixed(1)}%"></i>`)
    .join("");
  const marks = resetMarks(d.history, nowMs)
    .map((m) => `<s class="up-rs" style="left:${(m.x * 100).toFixed(1)}%"><span>${clock(m.at)}</span></s>`)
    .join("");
  const full = fullSpans(d.history, nowMs)
    .filter((x) => x.to > now - DAY)
    .map((x) => warn("high", t("usage.fullSpan", { from: clock(x.from), to: clock(x.to) })))
    .join("");
  let out = `<section class="up-sec"><h4>${t("usage.last24h")}</h4>${note}<div class="up-chart">${bars}${marks}</div><p class="up-axis"><span>${t("usage.axisDayAgo")}</span><span>${t("usage.axisNow")}</span></p>${full}</section>`;
  const week = u.windows.find((w) => w.label === "7d");
  if (week) {
    const days = weekDays(d.history, "7d", week.resetsAt);
    const models = u.models.map((m) => new Map(weekDays(d.history, m.name, m.resetsAt).map((x) => [x.dayStart, x.delta])));
    const even = 100 / 7;
    const scale = Math.max(even * 2, ...days.map((x) => x.delta), ...models.flatMap((m) => [...m.values()]));
    const w = (n: number) => `${((n / scale) * 100).toFixed(1)}%`;
    const rows = days
      .map((x) => {
        const name = new Date(x.dayStart * 1000).toLocaleDateString(language(), { weekday: "short" });
        const thin = models.map((m) => `<i class="up-wm" style="width:${w(m.get(x.dayStart) ?? 0)}"></i>`).join("");
        const value = x.dayStart > now ? "" : pct(x.delta);
        return `<div class="${x.dayStart > now ? "up-day later" : "up-day"}"><span>${name}</span><div class="up-track"><i class="${x.delta > even ? "up-wd high" : "up-wd"}" style="width:${w(x.delta)}"></i>${thin}<s class="up-even" style="left:${w(even)}"></s></div><span>${value}</span></div>`;
      })
      .join("");
    out += `<section class="up-sec"><h4>${t("usage.weekDays")}</h4>${rows}${line(`<span class="up-dim">${t("usage.evenShare", { n: Math.round(even) })}</span>`)}</section>`;
  }
  return out;
}

export interface PopoverOptions {
  u: Usage;
  details: UsageDetails | null;
  share: Share | null | "loading";
  rows: SessionView[];
  layout: "tabs" | "all";
  tab: "now" | "sessions" | "history";
  /** The window is narrower than the "all" layout: one column. */
  narrow: boolean;
  nowMs: number;
  /** HTML: "Limity planu · claude", with the account's name and color. */
  title: string;
}

/** The whole popover: header, the tabs or all three views, footer. */
export function renderPopover(o: PopoverOptions): string {
  const claude = o.u.agent === "claude";
  const loading = `<p class="up-line up-dim">${t("usage.loading")}</p>`;
  const models = o.u.models.map((m) => m.name);
  const sessions = () => (o.details ? renderSessions(o.rows, o.share, o.layout === "all", o.nowMs, models) : loading);
  const history = () => (o.details ? renderHistory(o.details, o.u, o.nowMs) : loading);
  const head = `<header class="up-head"><span class="up-title">${o.title}</span><span class="up-dim">${t("usage.readAt", { time: clock(o.nowMs / 1000) })}</span></header>`;
  let nav = "";
  let body: string;
  if (o.layout === "tabs") {
    const tab = !claude && o.tab === "sessions" ? "now" : o.tab;
    const state = chipState(o.u, o.nowMs);
    const button = (id: string, label: string) =>
      `<button role="tab" data-tab="${id}" aria-selected="${tab === id}">${label}</button>`;
    nav = `<nav class="up-tabs" role="tablist">${button("now", `${t("usage.tabNow")}${state.level ? `<i class="up-dot ${state.level}"></i>` : ""}`)}${claude ? button("sessions", `${t("usage.tabSessions")} <span class="up-count">${o.rows.length}</span>`) : ""}${button("history", t("usage.tabHistory"))}</nav>`;
    body = tab === "now" ? renderNow(o.u, o.nowMs) : tab === "sessions" ? sessions() : history();
  } else {
    body = `<div class="up-cols"><div>${renderNow(o.u, o.nowMs)}</div><div>${history()}</div></div>${claude ? `<div class="up-all-sessions"><h4>${t("usage.tabSessions")}</h4>${sessions()}</div>` : ""}`;
  }
  return `<div class="up ${o.layout}${o.narrow ? " narrow" : ""}" role="dialog" aria-label="${t("usage.popover")}"><i class="up-nib"></i>${head}${nav}<div class="up-body">${body}</div><footer class="up-foot">${t("usage.footer")}</footer></div>`;
}
