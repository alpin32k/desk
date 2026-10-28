// The side panel's file column: a project's folders and files, a name
// filter, and `?text` to search inside the files.

import { api } from "./api";
import { ancestors, buildTree, filterPaths, findNode, type TreeNode } from "./filetree";
export { itemsLabel } from "./filetree";
import { t } from "./i18n";
import { escapeHtml } from "./tile";

export interface TreeEvents {
  /** A file was picked: show it. */
  file(rel: string): void;
  /** A folder was picked ("" = the project itself). */
  folder(rel: string): void;
  /** Double click on a file: open it in an editor. */
  edit(rel: string): void;
}

export class FileTree {
  readonly el: HTMLElement;
  project: string | null = null;
  private filter: HTMLInputElement;
  private list: HTMLElement;
  private files: string[] = [];
  private root: TreeNode = buildTree([]);
  private expanded = new Set<string>();
  private selected: string | null = null;
  /** Results of a `?text` search, when one is shown. */
  private found: string[] | null = null;
  private searchTimer: number | undefined;
  private loading = 0;

  constructor(private on: TreeEvents) {
    this.el = document.createElement("div");
    this.el.className = "side-tree";
    this.el.innerHTML = `
      <input class="tree-filter" spellcheck="false" />
      <div class="tree-list"></div>`;
    this.filter = this.el.querySelector(".tree-filter")!;
    this.list = this.el.querySelector(".tree-list")!;
    this.filter.placeholder = t("tree.filter");
    this.filter.addEventListener("input", () => this.onFilter());
    this.filter.addEventListener("keydown", (e) => {
      if (e.key === "Escape") {
        if (this.filter.value) {
          this.filter.value = "";
          this.onFilter();
        } else this.filter.blur();
      } else if (e.key === "Enter") {
        const first = this.list.querySelector<HTMLElement>(".tree-row[data-file]");
        if (first) this.pick(first.dataset.path!, false);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        this.list.querySelector<HTMLElement>(".tree-row")?.focus();
      }
    });
    this.list.addEventListener("click", (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>(".tree-row");
      if (row) this.pick(row.dataset.path!, row.dataset.file === undefined);
    });
    this.list.addEventListener("dblclick", (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>(".tree-row[data-file]");
      if (row) this.on.edit(row.dataset.path!);
    });
    this.list.addEventListener("keydown", (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>(".tree-row");
      if (!row) return;
      const move = (to: Element | null) => (to as HTMLElement | null)?.focus();
      if (e.key === "ArrowDown") move(row.nextElementSibling);
      else if (e.key === "ArrowUp") move(row.previousElementSibling ?? this.filter);
      else if (e.key === "Enter") this.pick(row.dataset.path!, row.dataset.file === undefined);
      else if (e.key === "ArrowRight" && row.dataset.file === undefined && !this.expanded.has(row.dataset.path!)) {
        this.pick(row.dataset.path!, true);
      } else if (e.key === "ArrowLeft" && this.expanded.has(row.dataset.path!)) this.pick(row.dataset.path!, true);
      else return;
      e.preventDefault();
    });
  }

  /** Show `path`'s files; keeps the state when it is already shown. */
  async load(path: string | null, force = false): Promise<void> {
    if (path === this.project && !force) return;
    this.project = path;
    this.expanded.clear();
    this.selected = null;
    this.found = null;
    this.files = [];
    this.root = buildTree([]);
    this.render();
    if (!path) return;
    const ticket = ++this.loading;
    const files = await api.projectFiles(path).catch(() => []);
    if (ticket !== this.loading) return; // another project was picked meanwhile
    this.files = files;
    this.root = buildTree(files);
    this.render();
  }

  focusFilter(): void {
    this.filter.focus();
    this.filter.select();
  }

  /** Children count of a folder ("" = the project). */
  count(rel: string): number {
    return findNode(this.root, rel)?.children.length ?? 0;
  }

  /** Mark a file as the one shown, unfolding the tree down to it. */
  reveal(rel: string): void {
    for (const a of ancestors(rel)) this.expanded.add(a);
    this.selected = rel;
    this.render();
  }

  private pick(rel: string, dir: boolean): void {
    this.selected = rel;
    if (dir) {
      if (this.expanded.has(rel)) this.expanded.delete(rel);
      else this.expanded.add(rel);
      this.on.folder(rel);
    } else {
      this.on.file(rel);
    }
    this.render();
    this.list.querySelector<HTMLElement>(`.tree-row[data-path="${CSS.escape(rel)}"]`)?.focus();
  }

  private onFilter(): void {
    clearTimeout(this.searchTimer);
    const q = this.filter.value;
    if (q.startsWith("?")) {
      const text = q.slice(1).trim();
      if (!text || !this.project) {
        this.found = [];
        return this.render();
      }
      const project = this.project;
      this.searchTimer = window.setTimeout(async () => {
        const found = await api.searchFiles(project, text).catch(() => []);
        if (this.filter.value === q && this.project === project) {
          this.found = found;
          this.render();
        }
      }, 250);
      return;
    }
    this.found = null;
    this.render();
  }

  /** Labels in the current language (Options changed it). */
  relabel(): void {
    this.filter.placeholder = t("tree.filter");
    this.render();
  }

  private render(): void {
    const q = this.filter.value;
    if (!this.project) {
      this.list.innerHTML = `<div class="tree-empty dim">${t("tree.pickTile")}</div>`;
      return;
    }
    if (q.trim() && !q.startsWith("?")) return this.flat(filterPaths(this.files, q), t("tree.noMatch"));
    if (q.startsWith("?")) return this.flat(this.found ?? [], this.found ? t("tree.nothingFound") : t("tree.searching"));
    const rows: string[] = [];
    const walk = (nodes: TreeNode[], depth: number) => {
      for (const n of nodes) {
        rows.push(this.row(n.path, n.name, n.dir, depth));
        if (n.dir && this.expanded.has(n.path)) walk(n.children, depth + 1);
      }
    };
    walk(this.root.children, 0);
    this.list.innerHTML = rows.join("") || `<div class="tree-empty dim">${this.files.length ? "" : t("tree.loading")}</div>`;
  }

  private flat(paths: string[], empty: string): void {
    this.list.innerHTML =
      paths
        .map((p) => {
          const at = p.lastIndexOf("/");
          return this.row(p, p.slice(at + 1), false, 0, at > 0 ? p.slice(0, at) : "");
        })
        .join("") || `<div class="tree-empty dim">${empty}</div>`;
  }

  private row(path: string, name: string, dir: boolean, depth: number, where = ""): string {
    const open = dir && this.expanded.has(path);
    const cls = `tree-row${path === this.selected ? " sel" : ""}${dir ? " dir" : ""}${open ? " open" : ""}`;
    return `<div class="${cls}" tabindex="-1" data-path="${escapeHtml(path)}"${dir ? "" : " data-file"} style="padding-left:${8 + depth * 14}px">
      <span class="tree-icon">${dir ? "›" : ""}</span><span class="tree-name">${escapeHtml(name)}</span>${
        where ? `<span class="tree-where">${escapeHtml(where)}</span>` : ""
      }</div>`;
  }
}
