// CPU temperature in the top bar ("72°", "84° ↓" while desk slows its
// sessions, "78° ≋" while the fans are at full speed). The readings, the
// slowing and the fans are Rust's (src-tauri/src/thermal.rs, fans.rs).
import type { FanInfo, ThermalMode, ThermalStatus } from "./api";
import { t, tn, type MessageKey } from "./i18n";

export const DEFAULT_LIMIT = 75;
export const MIN_LIMIT = 50;
export const MAX_LIMIT = 100;
/** Full speed back this far below the limit (thermal.rs COOL_MARGIN). */
export const COOL_MARGIN = 7;
/** The chip turns yellow this close to the limit. */
const WARM_MARGIN = 10;
/** Where the fans start in "fansFirst", unless Options say otherwise (thermal.rs). */
export const DEFAULT_FAN_LIMIT = 70;

/** Options' order. */
export const THERMAL_MODES: ThermalMode[] = ["off", "slow", "fansFirst", "together", "fans"];

export const MODE_LABELS: Record<ThermalMode, MessageKey> = {
  off: "settings.thermalModeOff",
  slow: "settings.thermalModeSlow",
  fansFirst: "settings.thermalModeFansFirst",
  together: "settings.thermalModeTogether",
  fans: "settings.thermalModeFans",
};

export function usesFans(mode: ThermalMode): boolean {
  return mode === "fansFirst" || mode === "together" || mode === "fans";
}

export function slows(mode: ThermalMode): boolean {
  return mode === "slow" || mode === "fansFirst" || mode === "together";
}

/** A limit typed in Options: within MIN..MAX, whole degrees. */
export function clampLimit(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_LIMIT;
  return Math.min(MAX_LIMIT, Math.max(MIN_LIMIT, Math.round(n)));
}

/** The fan limit typed in Options: like the limit, and never above it. */
export function clampFanLimit(n: number, limit: number): number {
  return Math.min(Number.isFinite(n) ? clampLimit(n) : DEFAULT_FAN_LIMIT, limit);
}

/** Options' modes; `why`: a fan mode greyed out, and the reason. */
export function modeOptions(info: FanInfo | null): { mode: ThermalMode; why: string | null }[] {
  const why = info ? info.unsupported : t("fans.unknown");
  return THERMAL_MODES.map((mode) => ({ mode, why: usesFans(mode) ? why : null }));
}

/** Slowing actually happens: hot, and some processes were slowed. */
export function slowing(s: ThermalStatus | null): boolean {
  return !!s && s.slowed && s.processes > 0;
}

export function thermalLevel(s: ThermalStatus): "" | "warm" | "hot" {
  if (s.slowed || s.fans?.forced) return "hot";
  const active = [s.limit, s.fanLimit].filter((n) => n > 0);
  const limit = active.length ? Math.min(...active) : DEFAULT_LIMIT;
  if (s.celsius !== null && s.celsius >= limit) return "hot";
  if (s.celsius !== null && s.celsius >= limit - WARM_MARGIN) return "warm";
  return "";
}

export function thermalTitle(s: ThermalStatus): string {
  const c = s.celsius === null ? "?" : String(s.celsius);
  if (!s.limit && !s.fanLimit) return t("thermal.offTitle", { c });
  const lines = [slowTitle(s, c)];
  if (s.fanLimit) lines.push(fanLine(s));
  return lines.join("\n");
}

function slowTitle(s: ThermalStatus, c: string): string {
  if (!s.limit) return t("thermal.cpu", { c });
  if (!s.slowed) return t("thermal.title", { c, limit: s.limit });
  if (s.cannotSlow) return t("thermal.cannotTitle", { c, limit: s.limit, why: s.cannotSlow });
  return tn("thermal.slowedTitle", s.processes, { c, limit: s.limit, cool: s.limit - COOL_MARGIN });
}

function fanLine(s: ThermalStatus): string {
  if (s.fans?.problem) return t("thermal.fansProblemLine", { why: s.fans.problem });
  if (s.fans?.forced) {
    return t("thermal.fansForced", { rpm: s.fans.rpm ?? s.fans.maxRpm ?? "?", cool: s.fanLimit - COOL_MARGIN });
  }
  return t("thermal.fansReady", { limit: s.fanLimit });
}

/** What changed between two readings, as one toast (or none). */
export function thermalToast(prev: ThermalStatus | null, next: ThermalStatus): string | null {
  const c = next.celsius ?? "?";
  const slowOn = slowing(next) && !slowing(prev);
  // Turned off in Options is not "cooled down".
  const slowOff = slowing(prev) && !next.slowed && next.limit > 0;
  const wasFans = !!prev?.fans?.forced;
  const isFans = !!next.fans?.forced;
  const fansOn = isFans && !wasFans;
  const fansOff = wasFans && !isFans && next.fanLimit > 0;
  if (slowOn && fansOn) return t("thermal.slowingFans", { c });
  if (slowOff && fansOff) return t("thermal.cooledFans", { c });
  if (slowOn) return t("thermal.slowing", { c });
  if (fansOn) return t("thermal.fansOn", { c });
  if (slowOff) return t("thermal.cooled", { c });
  if (fansOff) return t("thermal.fansOff", { c });
  const why = next.fans?.problem;
  if (why && why !== prev?.fans?.problem) return t("thermal.fansProblem", { why });
  return null;
}

/** The chip; nothing on a computer without a readable sensor that is not slowing. */
export function thermalHtml(s: ThermalStatus | null): string {
  if (!s || (s.celsius === null && !s.slowed && !s.fans?.forced)) return "";
  const text = `${s.celsius === null ? "🌡" : `${s.celsius}°`}${s.fans?.forced ? " ≋" : ""}${slowing(s) ? " ↓" : ""}`;
  const cls = ["top-btn", "thermal", thermalLevel(s)].filter(Boolean).join(" ");
  return `<button class="${cls}" data-top="thermal" data-thermal title="${escapeHtml(thermalTitle(s))}">${text}</button>`;
}

/** Same as tile.ts's (not imported: that pulls the terminal into tests). */
function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}
