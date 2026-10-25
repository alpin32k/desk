// Palette list: fuzzy matching and worktrees grouped under their main repo.
import type { Project } from "./api";

export interface Row {
  project: Project;
  /** Main repo this worktree belongs to, when shown under it. */
  parent?: Project;
  score: number;
}

/** Subsequence match, higher is better; -1 when `query` does not match. */
export function fuzzyScore(query: string, text: string): number {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (!q) return 0;
  const direct = t.indexOf(q);
  if (direct >= 0) return 1000 - direct * 2 - (t.length - q.length) * 0.1;
  let score = 0;
  let ti = 0;
  let streak = 0;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return -1;
    streak = found === ti ? streak + 1 : 0;
    score += 10 + streak * 5 - (found - ti);
    ti = found + 1;
  }
  return score;
}

function rank(query: string, p: Project, parent?: Project): number {
  const byName = fuzzyScore(query, p.name);
  const byParent = parent ? fuzzyScore(query, parent.name) - 50 : -1;
  const byPath = fuzzyScore(query, p.path) - 200;
  return Math.max(byName, byParent, byPath);
}

/**
 * Without a query: projects by recent activity, each main repo followed by
 * its worktrees. With a query: best matches first, worktrees keep their parent label.
 */
export function rows(projects: Project[], query: string): Row[] {
  const byPath = new Map(projects.map((p) => [p.path, p]));
  const parentOf = (p: Project) => (p.worktreeOf ? byPath.get(p.worktreeOf) : undefined);

  if (query.trim()) {
    return projects
      .map((p) => ({ project: p, parent: parentOf(p), score: rank(query.trim(), p, parentOf(p)) }))
      .filter((r) => r.score >= 0)
      .sort((a, b) => b.score - a.score || b.project.lastActiveMs - a.project.lastActiveMs);
  }

  const children = new Map<string, Project[]>();
  for (const p of projects) {
    const parent = parentOf(p);
    if (parent) children.set(parent.path, [...(children.get(parent.path) ?? []), p]);
  }
  const out: Row[] = [];
  for (const p of projects) {
    if (parentOf(p)) continue;
    out.push({ project: p, score: 0 });
    for (const c of children.get(p.path) ?? []) out.push({ project: c, parent: p, score: 0 });
  }
  return out;
}

export function shortPath(path: string, home: string): string {
  return home && (path === home || path.startsWith(home + "/")) ? "~" + path.slice(home.length) : path;
}

export function ago(ms: number, now = Date.now()): string {
  if (!ms) return "";
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  return `${Math.floor(s / 86400)}d`;
}
