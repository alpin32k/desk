// Claude accounts (src-tauri/src/accounts.rs): each one is a Claude Code config
// folder with its own login, conversations and limits. The first one is the
// default (~/.claude). With fewer than two, desk shows no accounts anywhere.
import type { ClaudeAccount, Layout } from "./api";

/** One color per account, by its place in the list (not the project palette). */
export const ACCOUNT_COLORS = ["#e3b341", "#56d4dd", "#d2a8ff", "#7ee787", "#ff9bce", "#ffa657"];

type Accounts = Pick<Layout, "claudeAccounts" | "tileAccounts" | "projectAccounts">;

/** Two or more accounts: tiles and the top bar say which one they use. */
export function hasAccounts(l: Pick<Layout, "claudeAccounts">): boolean {
  return (l.claudeAccounts?.length ?? 0) > 1;
}

export function accountById(l: Pick<Layout, "claudeAccounts">, id: string | null | undefined): ClaudeAccount | undefined {
  const list = l.claudeAccounts ?? [];
  return list.find((a) => a.id === id) ?? list[0];
}

export function accountColor(l: Pick<Layout, "claudeAccounts">, id: string | null | undefined): string {
  const list = l.claudeAccounts ?? [];
  const at = Math.max(0, list.findIndex((a) => a.id === id));
  return ACCOUNT_COLORS[at % ACCOUNT_COLORS.length];
}

/** The account a Claude session `key` of project `path` runs on: the one it
 *  was started with, else the project's, else the default. "" without accounts. */
export function accountFor(l: Accounts, key: string, path: string): string {
  const known = (id: string | undefined) => (id && l.claudeAccounts?.some((a) => a.id === id) ? id : undefined);
  return known(l.tileAccounts?.[key]) ?? known(l.projectAccounts?.[path]) ?? l.claudeAccounts?.[0]?.id ?? "";
}

/** A short id for a new account, unique in the list. */
export function newAccountId(name: string, list: ClaudeAccount[]): string {
  const base =
    name
      .toLowerCase()
      .normalize("NFD")
      .replace(/[̀-ͯ]/g, "")
      .replace(/ł/g, "l")
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "konto";
  let id = base;
  for (let n = 2; list.some((a) => a.id === id); n++) id = `${base}-${n}`;
  return id;
}

/** Config folder suggested for a new account: ~/.claude-<id>. */
export function defaultDir(id: string): string {
  return `~/.claude-${id}`;
}

/** The list with an account added; the first one added also brings the default one. */
export function withAccount(list: ClaudeAccount[], name: string, dir: string, defaultName: string): ClaudeAccount[] {
  const base = list.length ? list : [{ id: "main", name: defaultName, dir: "" }];
  const id = newAccountId(name, base);
  return [...base, { id, name: name.trim() || id, dir: dir.trim() || defaultDir(id) }];
}

/** Forget an account (never the default one) and the tiles and projects set to it. */
export function withoutAccount(l: Accounts, id: string): Accounts {
  const list = l.claudeAccounts ?? [];
  if (!list.length || list[0].id === id) return l;
  const drop = (m: Record<string, string> = {}) => Object.fromEntries(Object.entries(m).filter(([, v]) => v !== id));
  return {
    claudeAccounts: list.filter((a) => a.id !== id),
    tileAccounts: drop(l.tileAccounts),
    projectAccounts: drop(l.projectAccounts),
  };
}
