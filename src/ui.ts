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
