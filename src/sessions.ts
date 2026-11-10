// A tile is one session: project path + agent + sub-session number. Its key is
// stored in layout.json; the main Claude session keeps the bare path so
// layouts from before agents existed still load.
import { tn } from "./i18n";

export interface Spec {
  path: string;
  agent: string;
  instance: number;
}

export function keyOf(s: Spec): string {
  return s.agent === "claude" && s.instance === 0 ? s.path : `${s.agent}#${s.instance}:${s.path}`;
}

export function parseKey(key: string): Spec {
  // The path is absolute: "/Users/..." or, on Windows, "C:\\..." / "\\\\server\\...".
  const m = /^([a-z0-9][a-z0-9_-]*)#(\d+):(\/.*|[a-z]:[\\/].*|\\\\.*)$/i.exec(key);
  return m ? { agent: m[1], instance: Number(m[2]), path: m[3] } : { path: key, agent: "claude", instance: 0 };
}

/** Next free sub-session number for `path` + `agent` among `taken`. */
export function nextInstance(taken: Spec[], path: string, agent: string): number {
  const used = new Set(taken.filter((s) => s.path === path && s.agent === agent).map((s) => s.instance));
  let n = 0;
  while (used.has(n)) n++;
  return n;
}

/** A tile and the lane it stood in. */
export interface Place {
  key: string;
  lane: number;
}

/**
 * Sessions that ended on their own, to tell one that ended alone (`exit`, the
 * agent quit) from many ending together: a restart or tmux going down takes
 * them all within seconds, and desk may be gone before it learns more. One
 * ending alone stays in the saved layout for `window` ms; once another ends
 * within that time, all of them are kept for the next start.
 */
export class Endings {
  private pending: (Place & { at: number })[] = [];
  private last = -Infinity;

  constructor(private readonly window: number) {}

  /** Session `key` (in `lane`) ended at `now`; returns the ones to keep for the next start. */
  ended(key: string, lane: number, now: number): Place[] {
    const together = now - this.last < this.window;
    this.last = now;
    this.pending.push({ key, lane, at: now });
    if (!together) return [];
    const kept = this.pending.map(({ key, lane }) => ({ key, lane }));
    this.pending = [];
    return kept;
  }

  /** Those that ended alone `window` ms ago are gone for good. */
  settle(now: number): void {
    this.pending = this.pending.filter((p) => now - p.at < this.window);
  }

  /** The tile is open again. */
  forget(key: string): void {
    this.pending = this.pending.filter((p) => p.key !== key);
  }

  /** Still in the saved layout. */
  get held(): Place[] {
    return this.pending.map(({ key, lane }) => ({ key, lane }));
  }
}

/** Header label: `claude`, `codex`, `claude #2` (sub-sessions count from 2 for people). */
export function agentLabel(s: Spec): string {
  return s.instance > 0 ? `${s.agent} #${s.instance + 1}` : s.agent;
}

/** "2 sesje", "5 sesji", "22 sesje" ("2 sessions" in English). */
export function sessionsLabel(n: number): string {
  return tn("card.sessions", n);
}

/** Sessions running without a tile, by project name, then agent and sub-session. */
export function background<T extends Spec>(sessions: T[], hasTile: (key: string) => boolean): T[] {
  const name = (s: Spec) => (s.path.split(/[\\/]/).pop() ?? s.path).toLowerCase();
  return sessions
    .filter((s) => !hasTile(keyOf(s)))
    .sort((a, b) => name(a).localeCompare(name(b)) || a.path.localeCompare(b.path) || a.agent.localeCompare(b.agent) || a.instance - b.instance);
}
