// Small UI helpers: confirm dialog and toasts (window.confirm is not available in the webview).
import type { GithubRepo } from "./api";
import { matchRepos, repoName } from "./github";
import { t } from "./i18n";
import { escapeHtml } from "./tile";

export function confirmBox(message: string, okLabel = t("common.yes")): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    overlay.innerHTML = `
      <div class="dialog confirm">
        <p>${escapeHtml(message)}</p>
        <div class="buttons"><button data-v="0">${t("common.cancel")} <kbd>esc</kbd></button><button data-v="1" class="danger">${escapeHtml(okLabel)} <kbd>↵</kbd></button></div>
      </div>`;
    const done = (v: boolean) => {
      window.removeEventListener("keydown", onKey, true);
      overlay.remove();
      resolve(v);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        done(e.key === "Enter");
      }
    };
    overlay.addEventListener("click", (e) => {
      const v = (e.target as HTMLElement).closest("button")?.dataset.v;
      if (v !== undefined) done(v === "1");
      else if (e.target === overlay) done(false);
    });
    window.addEventListener("keydown", onKey, true);
    document.body.appendChild(overlay);
  });
}

export interface Picked {
  value: string;
  /** Chosen with ⌥↵ / ⌥-click. */
  alt: boolean;
}

export interface PickOptions {
  current?: string | null;
  /** Footer hint for ⌥↵, when it does something. */
  altHint?: string;
  /** Extra text after an item (e.g. where a command comes from). */
  note?: (item: string) => string;
}

const PICK_ROWS = 300;

/** Pick one of `items` (type to filter, ↑↓, ↵); resolves to null on Esc. */
export function pickBox(title: string, items: string[], opts: PickOptions = {}): Promise<Picked | null> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    overlay.innerHTML = `
      <div class="dialog palette pick">
        <input type="text" spellcheck="false" autocomplete="off" />
        <div class="list"></div>
        <footer><span class="dim"></span><kbd>↵</kbd> ${t("pick.choose")} ${
          opts.altHint ? `<kbd>⌥↵</kbd> ${escapeHtml(opts.altHint)} ` : ""
        }<kbd>esc</kbd> ${t("common.close")}</footer>
      </div>`;
    const input = overlay.querySelector("input")!;
    const list = overlay.querySelector<HTMLElement>(".list")!;
    input.placeholder = title;
    overlay.querySelector("footer .dim")!.textContent = title;
    let shown: string[] = [];
    let sel = 0;
    const draw = () => {
      const q = input.value.toLowerCase();
      const words = q.split(/\s+/).filter(Boolean);
      shown = items.filter((i) => words.every((w) => i.toLowerCase().includes(w))).slice(0, PICK_ROWS);
      sel = Math.min(sel, Math.max(0, shown.length - 1));
      list.innerHTML = shown
        .map(
          (i, n) =>
            `<div class="row ${n === sel ? "sel" : ""}" data-i="${n}"><span class="name">${escapeHtml(i)}</span>${
              i === opts.current ? `<span class="dim">${t("pick.current")}</span>` : ""
            }${opts.note ? `<span class="dim">${escapeHtml(opts.note(i))}</span>` : ""}</div>`,
        )
        .join("");
      list.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
    };
    const done = (v: Picked | null) => {
      window.removeEventListener("keydown", onKey, true);
      overlay.remove();
      resolve(v);
    };
    const onKey = (e: KeyboardEvent) => {
      const keys: Record<string, () => void> = {
        Escape: () => done(null),
        Enter: () => done(shown[sel] === undefined ? null : { value: shown[sel], alt: e.altKey }),
        ArrowDown: () => ((sel = Math.min(sel + 1, shown.length - 1)), draw()),
        ArrowUp: () => ((sel = Math.max(sel - 1, 0)), draw()),
      };
      const f = keys[e.key];
      if (!f) return;
      e.preventDefault();
      e.stopPropagation();
      f();
    };
    input.addEventListener("input", () => ((sel = 0), draw()));
    overlay.addEventListener("click", (e) => {
      const i = (e.target as HTMLElement).closest<HTMLElement>(".row")?.dataset.i;
      if (i !== undefined) done({ value: shown[Number(i)], alt: e.altKey });
      else if (e.target === overlay) done(null);
    });
    window.addEventListener("keydown", onKey, true);
    document.body.appendChild(overlay);
    draw();
    input.focus();
  });
}

export function toast(message: string, ms = 4000): void {
  const el = document.createElement("div");
  el.className = "toast";
  el.textContent = message;
  document.body.appendChild(el);
  setTimeout(() => el.remove(), ms);
}

export interface NewProject {
  parent: string;
  name: string;
  git: boolean;
  /** `owner/name` to clone instead of creating an empty folder. */
  repo: string | null;
}

/** Ask for a new project's name and parent folder; typing a name also
 * offers matching GitHub repos to clone. */
export function newProjectBox(
  defaultParent: string,
  pickFolder: () => Promise<string | null>,
  agent: string,
  opts: { name?: string; repos: Promise<GithubRepo[]>; isLocal: (name: string) => boolean },
): Promise<NewProject | null> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    overlay.innerHTML = `
      <div class="dialog new-project">
        <h2>${t("newProject.title")} <span class="dim">· ${escapeHtml(agent)}</span></h2>
        <label class="line">${t("newProject.name")} <input type="text" data-k="name" spellcheck="false" autocomplete="off" placeholder="${t("newProject.namePlaceholder")}" /></label>
        <div class="gh-list"></div>
        <label class="line">${t("newProject.parent")} <input type="text" data-k="parent" spellcheck="false" /><button data-k="pick">${t("newProject.pick")}</button></label>
        <label class="line" data-k="git-line"><input type="checkbox" data-k="git" checked /> ${t("newProject.git")}</label>
        <div class="line gh-chosen" hidden>${t("newProject.clone")} <b data-k="repo"></b><button data-k="unpick" title="${t("newProject.unpick")}">✕</button></div>
        <div class="buttons"><button data-v="0">${t("common.cancel")} <kbd>esc</kbd></button><button data-v="1"><span data-k="ok">${t("newProject.create")}</span> <kbd>↵</kbd></button></div>
      </div>`;
    const $ = <T extends HTMLElement>(k: string) => overlay.querySelector<T>(`[data-k="${k}"]`)!;
    const name = $<HTMLInputElement>("name");
    const parent = $<HTMLInputElement>("parent");
    const git = $<HTMLInputElement>("git");
    const list = overlay.querySelector<HTMLElement>(".gh-list")!;
    const chosenLine = overlay.querySelector<HTMLElement>(".gh-chosen")!;
    parent.value = defaultParent;
    name.value = opts.name ?? "";

    let repos: GithubRepo[] | null = null;
    let error = "";
    let shown: GithubRepo[] = [];
    let sel = -1;
    let chosen: GithubRepo | null = null;
    const draw = () => {
      chosenLine.hidden = !chosen;
      $("git-line").hidden = !!chosen;
      $("repo").textContent = chosen?.fullName ?? "";
      $("ok").textContent = chosen ? t("newProject.cloneOpen") : t("newProject.create");
      shown = chosen || !repos ? [] : matchRepos(repos, name.value);
      sel = Math.min(sel, shown.length - 1);
      const status =
        chosen || !name.value.trim()
          ? ""
          : error
            ? `<div class="gh-status">GitHub: ${escapeHtml(error)}</div>`
            : !repos
              ? `<div class="gh-status">GitHub: ${t("newProject.loadingRepos")}</div>`
              : "";
      list.innerHTML =
        status +
        shown
          .map(
            (r, i) => `<div class="row ${i === sel ? "sel" : ""}" data-i="${i}">
              <span class="name">${escapeHtml(r.fullName)}</span>
              <span class="desc">${escapeHtml(r.description ?? "")}</span>
              ${opts.isLocal(repoName(r)) ? `<span class="tag">${t("newProject.local")}</span>` : ""}
              ${r.private ? `<span class="tag">${t("common.private")}</span>` : ""}</div>`,
          )
          .join("");
    };
    const choose = (repo: GithubRepo | null) => {
      chosen = repo;
      sel = -1;
      if (repo) name.value = repoName(repo);
      draw();
      name.focus();
    };
    opts.repos.then(
      (r) => {
        repos = r;
        draw();
      },
      (err) => {
        error = String(err);
        draw();
      },
    );
    name.addEventListener("input", () => {
      sel = -1;
      draw();
    });
    const done = (ok: boolean) => {
      window.removeEventListener("keydown", onKey, true);
      overlay.remove();
      resolve(
        ok && name.value.trim()
          ? { parent: parent.value.trim(), name: name.value.trim(), git: git.checked, repo: chosen?.fullName ?? null }
          : null,
      );
    };
    const onKey = (e: KeyboardEvent) => {
      const move = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
      if (move && shown.length && document.activeElement === name) {
        e.preventDefault();
        sel = sel + move < -1 ? shown.length - 1 : sel + move >= shown.length ? -1 : sel + move;
        draw();
      } else if (e.key === "Enter" && sel >= 0 && shown[sel]) {
        e.preventDefault();
        e.stopPropagation();
        choose(shown[sel]!);
      } else if (e.key === "Escape" || e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        done(e.key === "Enter");
      }
    };
    overlay.addEventListener("click", async (e) => {
      const btn = (e.target as HTMLElement).closest<HTMLElement>("button");
      const row = (e.target as HTMLElement).closest<HTMLElement>(".gh-list [data-i]");
      if (row) choose(shown[Number(row.dataset.i)] ?? null);
      else if (btn?.dataset.k === "unpick") choose(null);
      else if (btn?.dataset.k === "pick") {
        const picked = await pickFolder();
        if (picked) parent.value = picked;
      } else if (btn?.dataset.v !== undefined) done(btn.dataset.v === "1");
      else if (e.target === overlay) done(false);
    });
    window.addEventListener("keydown", onKey, true);
    document.body.appendChild(overlay);
    draw();
    name.focus();
  });
}
