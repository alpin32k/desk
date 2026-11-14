// Plan usage per harness for the top bar ("claude 5h 42% · 7d 18%").
// The numbers come from the agents' own local files (see src-tauri/src/usage.rs).
import type { Usage } from "./api";
import { t } from "./i18n";
import { level } from "./usage-stats";

export { level };

export function resetsIn(resetsAt: number, nowMs: number): string {
  const min = Math.max(0, Math.round((resetsAt * 1000 - nowMs) / 60000));
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  if (h < 48) return `${h} h ${min % 60} min`;
  return `${Math.floor(h / 24)} d ${h % 24} h`;
}

/** Compact time to reset for the chip: "45m", "2h13m", "3d4h". */
export function shortReset(resetsAt: number, nowMs: number): string {
  const min = Math.max(0, Math.round((resetsAt * 1000 - nowMs) / 60000));
  if (min < 60) return `${min}m`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h}h${String(min % 60).padStart(2, "0")}m`;
  return `${Math.floor(h / 24)}d${h % 24}h`;
}

/** Same as tile.ts's (not imported: that pulls the terminal into tests). */
function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
}

/** How the top bar names a Claude account (its name and color, accounts.ts). */
export type AccountLabel = (id: string) => { name: string; color: string } | null;

export function usageHtml(list: Usage[], nowMs: number, account: AccountLabel = () => null): string {
  return list
    .filter((u) => u.windows.length)
    .map((u) => {
      const worst = Math.max(...u.windows.map((w) => w.usedPercent));
      const parts = u.windows
        .map((w) => `${w.label} ${Math.round(w.usedPercent)}% <span class="reset">↻${shortReset(w.resetsAt, nowMs)}</span>`)
        .join(" · ");
      const title = u.windows
        .map((w) => t("usage.window", { label: w.label, percent: w.usedPercent.toFixed(1), time: resetsIn(w.resetsAt, nowMs) }))
        .join("\n");
      const a = u.account ? account(u.account) : null;
      if (a) {
        const name = escapeHtml(a.name);
        return `<span class="usage ${level(worst)}" title="${t("usage.limitAccount", { account: name })}\n${title}"><i class="acct" style="background:${a.color}"></i><b>${name}</b> ${parts}</span>`;
      }
      return `<span class="usage ${level(worst)}" title="${t("usage.limit", { agent: u.agent })}\n${title}">${u.agent} ${parts}</span>`;
    })
    .join("");
}
