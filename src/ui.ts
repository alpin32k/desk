// Small UI helpers: confirm dialog and toasts (window.confirm is not available in the webview).
import { escapeHtml } from "./tile";

export function confirmBox(message: string, okLabel = "Tak"): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    overlay.innerHTML = `
      <div class="dialog confirm">
        <p>${escapeHtml(message)}</p>
        <div class="buttons"><button data-v="0">Anuluj <kbd>esc</kbd></button><button data-v="1" class="danger">${escapeHtml(okLabel)} <kbd>↵</kbd></button></div>
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
        <footer><span class="dim"></span><kbd>↵</kbd> wybierz ${
          opts.altHint ? `<kbd>⌥↵</kbd> ${escapeHtml(opts.altHint)} ` : ""
        }<kbd>esc</kbd> zamknij</footer>
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
              i === opts.current ? `<span class="dim">obecny</span>` : ""
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

export function toast(message: string): void {
  const el = document.createElement("div");
  el.className = "toast";
  el.textContent = message;
  document.body.appendChild(el);
  setTimeout(() => el.remove(), 4000);
}

export interface NewProject {
  parent: string;
  name: string;
  git: boolean;
}

/** Ask for a new project's name and parent folder. */
export function newProjectBox(
  defaultParent: string,
  pickFolder: () => Promise<string | null>,
  agent: string,
): Promise<NewProject | null> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    overlay.innerHTML = `
      <div class="dialog new-project">
        <h2>Nowy projekt <span class="dim">· ${escapeHtml(agent)}</span></h2>
        <label class="line">Nazwa <input type="text" data-k="name" spellcheck="false" placeholder="moj-projekt" /></label>
        <label class="line">W folderze <input type="text" data-k="parent" spellcheck="false" /><button data-k="pick">Wybierz…</button></label>
        <label class="line"><input type="checkbox" data-k="git" checked /> git init (branch main)</label>
        <div class="buttons"><button data-v="0">Anuluj <kbd>esc</kbd></button><button data-v="1">Utwórz i otwórz <kbd>↵</kbd></button></div>
      </div>`;
    const name = overlay.querySelector<HTMLInputElement>('[data-k="name"]')!;
    const parent = overlay.querySelector<HTMLInputElement>('[data-k="parent"]')!;
    const git = overlay.querySelector<HTMLInputElement>('[data-k="git"]')!;
    parent.value = defaultParent;
    const done = (ok: boolean) => {
      window.removeEventListener("keydown", onKey, true);
      overlay.remove();
      resolve(ok && name.value.trim() ? { parent: parent.value.trim(), name: name.value.trim(), git: git.checked } : null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        done(e.key === "Enter");
      }
    };
    overlay.addEventListener("click", async (e) => {
      const btn = (e.target as HTMLElement).closest<HTMLElement>("button");
      if (btn?.dataset.k === "pick") {
        const picked = await pickFolder();
        if (picked) parent.value = picked;
      } else if (btn?.dataset.v !== undefined) done(btn.dataset.v === "1");
      else if (e.target === overlay) done(false);
    });
    window.addEventListener("keydown", onKey, true);
    document.body.appendChild(overlay);
    name.focus();
  });
}
