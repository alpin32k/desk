// The side panel's file tree: built from the project's file list
// (`git ls-files` or a walk), filtered by name.
import { tn } from "./i18n";

export interface TreeNode {
  name: string;
  /** Relative to the project, "/"-separated; "" for the root. */
  path: string;
  dir: boolean;
  children: TreeNode[];
}

/** Folders first, then files, each by name (dotfiles first, like Finder). */
function sortNodes(nodes: TreeNode[]): void {
  nodes.sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name, undefined, { numeric: true }));
  for (const n of nodes) if (n.dir) sortNodes(n.children);
}

export function buildTree(paths: string[]): TreeNode {
  const root: TreeNode = { name: "", path: "", dir: true, children: [] };
  const dirs = new Map<string, TreeNode>([["", root]]);
  for (const p of paths) {
    const parts = p.split("/").filter(Boolean);
    let parent = root;
    for (let i = 0; i < parts.length; i++) {
      const path = parts.slice(0, i + 1).join("/");
      const last = i === parts.length - 1;
      if (last) {
        parent.children.push({ name: parts[i], path, dir: false, children: [] });
      } else {
        let dir = dirs.get(path);
        if (!dir) {
          dir = { name: parts[i], path, dir: true, children: [] };
          dirs.set(path, dir);
          parent.children.push(dir);
        }
        parent = dir;
      }
    }
  }
  sortNodes(root.children);
  return root;
}

export function findNode(root: TreeNode, path: string): TreeNode | null {
  if (!path) return root;
  let node: TreeNode | undefined = root;
  for (const part of path.split("/")) {
    node = node?.children.find((c) => c.name === part);
    if (!node) return null;
  }
  return node;
}

/**
 * Paths matching every word of `query` (case-insensitive), best first: a hit
 * in the file name beats one only in the folders, then shorter paths.
 */
export function filterPaths(paths: string[], query: string, limit = 200): string[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const scored: [number, string][] = [];
  for (const p of paths) {
    const lower = p.toLowerCase();
    if (!words.every((w) => lower.includes(w))) continue;
    const name = lower.slice(lower.lastIndexOf("/") + 1);
    const inName = words.filter((w) => name.includes(w)).length;
    scored.push([inName * 1000 - p.length, p]);
  }
  scored.sort((a, b) => b[0] - a[0] || a[1].localeCompare(b[1]));
  return scored.slice(0, limit).map(([, p]) => p);
}

/** Every folder on the way to `path` (to expand the tree down to it). */
export function ancestors(path: string): string[] {
  const parts = path.split("/").filter(Boolean);
  return parts.slice(0, -1).map((_, i) => parts.slice(0, i + 1).join("/"));
}

/** "1 element", "3 elementy", "12 elementów" ("3 items" in English). */
export function itemsLabel(n: number): string {
  return tn("side.items", n);
}
