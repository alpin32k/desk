// CPU temperature in the top bar ("72°", "84° ↓" while desk slows its
// sessions). The readings and the slowing are Rust's (src-tauri/src/thermal.rs).
import type { ThermalStatus } from "./api";
import { t, tn } from "./i18n";

export const DEFAULT_LIMIT = 75;
export const MIN_LIMIT = 50;
export const MAX_LIMIT = 100;
/** Full speed back this far below the limit (thermal.rs COOL_MARGIN). */
export const COOL_MARGIN = 7;
/** The chip turns yellow this close to the limit. */
const WARM_MARGIN = 10;

/** A limit typed in Options: within MIN..MAX, whole degrees. */
export function clampLimit(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_LIMIT;
  return Math.min(MAX_LIMIT, Math.max(MIN_LIMIT, Math.round(n)));
}

/** Slowing actually happens: hot, and some processes were slowed. */
export function slowing(s: ThermalStatus | null): boolean {
  return !!s && s.slowed && s.processes > 0;
}

export function thermalLevel(s: ThermalStatus): "" | "warm" | "hot" {
  if (s.slowed) return "hot";
  const limit = s.limit || DEFAULT_LIMIT;
  if (s.celsius !== null && s.celsius >= limit) return "hot";
  if (s.celsius !== null && s.celsius >= limit - WARM_MARGIN) return "warm";
  return "";
}

export function thermalTitle(s: ThermalStatus): string {
  const c = s.celsius === null ? "?" : String(s.celsius);
  if (!s.limit) return t("thermal.offTitle", { c });
  if (!s.slowed) return t("thermal.title", { c, limit: s.limit });
  if (s.cannotSlow) return t("thermal.cannotTitle", { c, limit: s.limit, why: s.cannotSlow });
  return tn("thermal.slowedTitle", s.processes, { c, limit: s.limit, cool: s.limit - COOL_MARGIN });
}

/** The chip; nothing on a computer without a readable sensor that is not slowing. */
export function thermalHtml(s: ThermalStatus | null): string {
  if (!s || (s.celsius === null && !s.slowed)) return "";
  const text = `${s.celsius === null ? "🌡" : `${s.celsius}°`}${slowing(s) ? " ↓" : ""}`;
  const cls = ["top-btn", "thermal", thermalLevel(s)].filter(Boolean).join(" ");
  return `<button class="${cls}" data-top="thermal" data-thermal title="${escapeHtml(thermalTitle(s))}">${text}</button>`;
}

/** Same as tile.ts's (not imported: that pulls the terminal into tests). */
function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}
