// GitHub repos offered in the new project dialog.
import type { GithubRepo } from "./api";

/** The part after `owner/`: the default folder name. */
export function repoName(repo: GithubRepo): string {
  return repo.fullName.slice(repo.fullName.indexOf("/") + 1);
}

/** Repos whose `owner/name` contains every word of the query; name prefixes
 * first, otherwise GitHub's order (most recently pushed). */
export function matchRepos(repos: GithubRepo[], query: string, limit = 6): GithubRepo[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const hits = repos.filter((r) => words.every((w) => r.fullName.toLowerCase().includes(w)));
  const prefix = (r: GithubRepo) => (repoName(r).toLowerCase().startsWith(words[0]!) ? 0 : 1);
  return hits
    .map((r, i) => ({ r, i }))
    .sort((a, b) => prefix(a.r) - prefix(b.r) || a.i - b.i)
    .slice(0, limit)
    .map(({ r }) => r);
}
