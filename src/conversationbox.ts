// ⌘S: a project's Claude conversations, the saved ones (by name) on top.
// ↵ picks one, ⌘↵ or ✎ names the selected one (saving it), ⌘⌫ or × forgets
// its name; the conversation itself stays on the list below.

import type { Conversation } from "./api";
import { t } from "./i18n";
import { keysIn } from "./keys";
import { PLATFORM, shortcut } from "./platform";
import { ago } from "./projects";
import { conversationRows, matchesRow, type ConversationRow } from "./saved";
import { escapeHtml } from "./tile";

export interface ConversationBoxOptions {
  /** The saved names of the project's conversations, by id. */
  names: Record<string, string>;
  /** Shown in a tile right now. */
  isOpen: (id: string) => boolean;
  /** Save under a name, or forget the name with "". */
  onName: (id: string, name: string) => void;
  /** The Claude account it is on, named when there are two or more. */
  account?: (c: Conversation) => string | null;
}

/** Resolves to the conversation picked, or null on Esc. */
export function conversationBox(title: string, found: Conversation[], opts: ConversationBoxOptions): Promise<Conversation | null> {
  return new Promise((resolve) => {
    const names = { ...opts.names };
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    overlay.innerHTML = `
      <div class="dialog palette pick conversations">
        <input type="text" spellcheck="false" autocomplete="off" />
        <div class="list"></div>
        <footer><span class="dim"></span><kbd>↵</kbd> ${t("pick.choose")} <kbd>${shortcut("⌘↵")}</kbd> ${t("conv.name")} <kbd>${shortcut("⌘⌫")}</kbd> ${t("conv.forget")} <kbd>esc</kbd> ${t("common.close")}</footer>
      </div>`;
    const input = overlay.querySelector<HTMLInputElement>("input")!;
    const list = overlay.querySelector<HTMLElement>(".list")!;
    input.placeholder = title;
    overlay.querySelector("footer .dim")!.textContent = title;
    let shown: ConversationRow[] = [];
    let sel = 0;
    let editing: HTMLInputElement | null = null;

    const row = (r: ConversationRow, n: number): string => {
      const c = r.c;
      const label = r.name ?? c.title ?? c.prompt;
      // Under a name: what Claude called it; else the first message, when the title is not it.
      const claude = c.title && c.title !== r.name ? c.title : c.prompt;
      const second = r.name ? claude : c.title ? c.prompt : "";
      const note = [opts.isOpen(c.id) ? t("app.conversationOpen") : "", opts.account?.(c) ?? "", ago(c.modifiedMs), c.branch ?? ""]
        .filter(Boolean)
        .join(" · ");
      const forget = r.name ? `<button class="conv-act" data-act="forget" title="${keysIn(t("conv.forgetTitle"))}">×</button>` : "";
      return `<div class="row${n === sel ? " sel" : ""}${r.name ? " saved" : ""}" data-i="${n}"><span class="name">${escapeHtml(label)}</span><span class="path">${escapeHtml(second)}</span><span class="dim">${escapeHtml(note)}</span><button class="conv-act" data-act="name" title="${keysIn(t("conv.nameTitle"))}">✎</button>${forget}</div>`;
    };

    const draw = () => {
      const rows = conversationRows(found, names).filter((r) => matchesRow(r, input.value));
      shown = rows;
      sel = Math.min(sel, Math.max(0, shown.length - 1));
      const savedCount = shown.filter((r) => r.name !== null).length;
      const parts = shown.map(row);
      // Headings only when something is saved: without, it is the plain recent list.
      if (savedCount) {
        parts.splice(savedCount, 0, savedCount < shown.length ? `<div class="section">${t("conv.recent")}</div>` : "");
        parts.unshift(`<div class="section">${t("conv.saved")}</div>`);
      }
      list.innerHTML = parts.join("");
      list.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
    };

    const done = (c: Conversation | null) => {
      window.removeEventListener("keydown", onKey, true);
      overlay.remove();
      resolve(c);
    };

    const setName = (r: ConversationRow, name: string) => {
      const trimmed = name.trim();
      if (trimmed) names[r.c.id] = trimmed;
      else delete names[r.c.id];
      opts.onName(r.c.id, trimmed);
      // The selection follows the row to its new place.
      const rows = conversationRows(found, names).filter((x) => matchesRow(x, input.value));
      sel = Math.max(0, rows.findIndex((x) => x.c.id === r.c.id));
      draw();
    };

    /** The selected row's name becomes a field: ↵ saves, Esc leaves it. */
    const rename = () => {
      const r = shown[sel];
      const el = list.querySelector<HTMLElement>(`.row[data-i="${sel}"] .name`);
      if (!r || !el) return;
      const field = document.createElement("input");
      field.className = "rename";
      field.spellcheck = false;
      field.placeholder = t("tile.namePlaceholder");
      field.value = r.name ?? r.c.title ?? "";
      el.replaceChildren(field);
      editing = field;
      field.focus();
      field.select();
      const finish = (save: boolean) => {
        if (editing !== field) return;
        editing = null;
        if (save) setName(r, field.value);
        else draw();
        input.focus();
      };
      field.addEventListener("keydown", (e) => {
        if (e.key !== "Enter" && e.key !== "Escape") return;
        e.preventDefault();
        e.stopPropagation();
        finish(e.key === "Enter");
      });
      field.addEventListener("blur", () => finish(true));
      field.addEventListener("click", (e) => e.stopPropagation());
    };

    const forget = () => {
      const r = shown[sel];
      if (r?.name) setName(r, "");
    };

    const mod = (e: KeyboardEvent) => (PLATFORM === "mac" ? e.metaKey : e.ctrlKey);
    const onKey = (e: KeyboardEvent) => {
      if (editing) return; // the name field handles its own keys
      const keys: Record<string, () => void> = {
        Escape: () => done(null),
        Enter: () => (mod(e) ? rename() : done(shown[sel]?.c ?? null)),
        ArrowDown: () => ((sel = Math.min(sel + 1, shown.length - 1)), draw()),
        ArrowUp: () => ((sel = Math.max(sel - 1, 0)), draw()),
      };
      const f = e.key === "Backspace" && mod(e) ? forget : keys[e.key];
      if (!f) return;
      e.preventDefault();
      e.stopPropagation();
      f();
    };
    input.addEventListener("input", () => ((sel = 0), draw()));
    overlay.addEventListener("click", (e) => {
      const target = e.target as HTMLElement;
      const i = target.closest<HTMLElement>(".row")?.dataset.i;
      if (i !== undefined) {
        sel = Number(i);
        const act = target.closest<HTMLElement>(".conv-act")?.dataset.act;
        if (act === "name") return (draw(), rename());
        if (act === "forget") return forget();
        if (!editing) done(shown[sel]?.c ?? null);
      } else if (e.target === overlay) done(null);
    });
    window.addEventListener("keydown", onKey, true);
    document.body.appendChild(overlay);
    draw();
    input.focus();
  });
}
