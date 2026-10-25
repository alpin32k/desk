// Plan usage per harness for the top bar ("claude 5h 42% · 7d 18%").
// The numbers come from the agents' own local files (see src-tauri/src/usage.rs).
import type { Usage } from "./api";

export function level(percent: number): "" | "warn" | "high" {
  if (percent >= 90) return "high";
  if (percent >= 70) return "warn";
  return "";
}

export function resetsIn(resetsAt: number, nowMs: number): string {
  const min = Math.max(0, Math.round((resetsAt * 1000 - nowMs) / 60000));
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  if (h < 48) return `${h} h ${min % 60} min`;
  return `${Math.floor(h / 24)} d ${h % 24} h`;
}

export function usageHtml(list: Usage[], nowMs: number): string {
  return list
    .filter((u) => u.windows.length)
    .map((u) => {
      const worst = Math.max(...u.windows.map((w) => w.usedPercent));
      const parts = u.windows.map((w) => `${w.label} ${Math.round(w.usedPercent)}%`).join(" · ");
      const title = u.windows
        .map((w) => `${w.label}: ${w.usedPercent.toFixed(1)}%, reset za ${resetsIn(w.resetsAt, nowMs)}`)
        .join("\n");
      return `<span class="usage ${level(worst)}" title="Limit planu ${u.agent}\n${title}">${u.agent} ${parts}</span>`;
    })
    .join("");
}
