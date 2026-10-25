// A tile is one session: project path + agent + sub-session number. Its key is
// stored in layout.json; the main Claude session keeps the bare path so
// layouts from before agents existed still load.
export interface Spec {
  path: string;
  agent: string;
  instance: number;
}

export function keyOf(s: Spec): string {
  return s.agent === "claude" && s.instance === 0 ? s.path : `${s.agent}#${s.instance}:${s.path}`;
}

export function parseKey(key: string): Spec {
  const m = /^([a-z0-9][a-z0-9_-]*)#(\d+):(\/.*)$/i.exec(key);
  return m ? { agent: m[1], instance: Number(m[2]), path: m[3] } : { path: key, agent: "claude", instance: 0 };
}

/** Next free sub-session number for `path` + `agent` among `taken`. */
export function nextInstance(taken: Spec[], path: string, agent: string): number {
  const used = new Set(taken.filter((s) => s.path === path && s.agent === agent).map((s) => s.instance));
  let n = 0;
  while (used.has(n)) n++;
  return n;
}

/** Header label: `claude`, `codex`, `claude #2` (sub-sessions count from 2 for people). */
export function agentLabel(s: Spec): string {
  return s.instance > 0 ? `${s.agent} #${s.instance + 1}` : s.agent;
}

/** "2 sesje", "5 sesji", "22 sesje". */
export function sessionsLabel(n: number): string {
  const few = n % 10 >= 2 && n % 10 <= 4 && (n % 100 < 12 || n % 100 > 14);
  return `${n} ${n === 1 ? "sesja" : few ? "sesje" : "sesji"}`;
}
