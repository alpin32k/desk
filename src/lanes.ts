// Pure operations on lanes: columns of session keys, top to bottom.
// Sessions of one project stay together (one card per project), so most
// operations take `of`, which maps a key to its project.
export type Lanes = string[][];
export type ProjectOf = (key: string) => string;

export function clone(lanes: Lanes): Lanes {
  return lanes.map((l) => [...l]);
}

export function flatten(lanes: Lanes): string[] {
  return lanes.flat();
}

export function contains(lanes: Lanes, path: string): boolean {
  return lanes.some((l) => l.includes(path));
}

export function shortestLane(lanes: Lanes): number {
  let best = 0;
  lanes.forEach((l, i) => {
    if (l.length < lanes[best].length) best = i;
  });
  return best;
}

/** Add a key after the other sessions of its project, or to the shortest lane. */
export function add(lanes: Lanes, path: string, of?: ProjectOf): Lanes {
  if (contains(lanes, path)) return lanes;
  const next = clone(lanes.length ? lanes : [[]]);
  if (of) {
    const project = of(path);
    for (const lane of next) {
      const last = lane.map(of).lastIndexOf(project);
      if (last >= 0) {
        lane.splice(last + 1, 0, path);
        return next;
      }
    }
  }
  next[shortestLane(next)].push(path);
  return next;
}

/** Runs of consecutive keys of one project in a lane. */
export function groups(lane: string[], of: ProjectOf): string[][] {
  const out: string[][] = [];
  for (const key of lane) {
    const last = out[out.length - 1];
    if (last && of(last[0]) === of(key)) last.push(key);
    else out.push([key]);
  }
  return out;
}

/** Pull every project's sessions together, where the project first appears. */
export function group(lanes: Lanes, of: ProjectOf): Lanes {
  let next: Lanes = lanes.map(() => []);
  lanes.forEach((lane, i) => {
    for (const key of lane) {
      const project = of(key);
      const home = next.findIndex((l) => l.some((k) => of(k) === project));
      if (home < 0) next[i].push(key);
      else next = add(next, key, of);
    }
  });
  return next;
}

/** Move a project's whole card to `lane`, before its `index`-th card (counted without it). */
export function moveGroup(lanes: Lanes, project: string, of: ProjectOf, lane: number, index: number): Lanes {
  if (lane < 0 || lane >= lanes.length) return lanes;
  const keys = lanes.flat().filter((k) => of(k) === project);
  if (!keys.length) return lanes;
  const next = lanes.map((l) => l.filter((k) => of(k) !== project));
  const cards = groups(next[lane], of);
  const at = Math.max(0, Math.min(index, cards.length));
  next[lane] = [...cards.slice(0, at).flat(), ...keys, ...cards.slice(at).flat()];
  return next;
}

export function remove(lanes: Lanes, path: string): Lanes {
  return lanes.map((l) => l.filter((p) => p !== path));
}

/** Move `path` to `lane` at `index` (index counted without the moved tile). */
export function move(lanes: Lanes, path: string, lane: number, index: number): Lanes {
  if (lane < 0 || lane >= lanes.length) return lanes;
  const next = remove(lanes, path);
  const target = next[lane];
  target.splice(Math.max(0, Math.min(index, target.length)), 0, path);
  return next;
}

/** Change the number of lanes; tiles from dropped lanes go to the shortest remaining ones. */
export function setLaneCount(lanes: Lanes, count: number): Lanes {
  const n = Math.max(1, Math.min(8, Math.round(count)));
  let next = clone(lanes).slice(0, n);
  while (next.length < n) next.push([]);
  for (const path of lanes.slice(n).flat()) next = add(next, path);
  return next;
}

/**
 * Tiles to show at startup: every tile from the saved layout (its session is
 * reattached, or started again if tmux lost it) plus autostart projects.
 */
export function restore(saved: Lanes, autostart: string[], laneCount: number): Lanes {
  let lanes = setLaneCount(saved.length ? saved : [[]], Math.max(laneCount, 1));
  for (const path of autostart) lanes = add(lanes, path);
  return lanes;
}

// ---- automatic layout ------------------------------------------------------

export interface AutoOptions {
  /** Tile width / height that reads best: terminals want lines, so about square. */
  aspect: number;
  /** Narrowest column worth having (CSS px, ~60 terminal columns). */
  minWidth: number;
  maxColumns: number;
}

export const AUTO: AutoOptions = { aspect: 1, minWidth: 480, maxColumns: 6 };

/**
 * Split cards (their session counts, in order) into `columns` contiguous
 * groups so the busiest column holds as few sessions as possible.
 * Returns how many cards go into each column.
 */
export function distribute(sizes: number[], columns: number): number[] {
  const n = sizes.length;
  const c = Math.max(1, Math.min(columns, n));
  const prefix = [0];
  for (const s of sizes) prefix.push(prefix[prefix.length - 1] + s);
  // best[k][i]: smallest possible tallest column for the first i cards in k columns.
  const best = Array.from({ length: c + 1 }, () => Array<number>(n + 1).fill(Infinity));
  const cut = Array.from({ length: c + 1 }, () => Array<number>(n + 1).fill(0));
  best[0][0] = 0;
  for (let k = 1; k <= c; k++) {
    for (let i = k; i <= n; i++) {
      for (let j = k - 1; j < i; j++) {
        const v = Math.max(best[k - 1][j], prefix[i] - prefix[j]);
        if (v < best[k][i]) {
          best[k][i] = v;
          cut[k][i] = j;
        }
      }
    }
  }
  const counts: number[] = [];
  for (let k = c, i = n; k > 0; k--) {
    const j = cut[k][i];
    counts.unshift(i - j);
    i = j;
  }
  return counts;
}

/** How many columns suit these cards in a `width`×`height` grid. */
export function autoColumns(sizes: number[], width: number, height: number, o: AutoOptions = AUTO): number {
  let bestColumns = 1;
  let bestScore = Infinity;
  for (let c = 1; c <= Math.min(sizes.length, o.maxColumns); c++) {
    const w = width / c;
    if (c > 1 && w < o.minWidth) break;
    const counts = distribute(sizes, c);
    let at = 0;
    let tallest = 0;
    for (const k of counts) {
      tallest = Math.max(tallest, sizes.slice(at, at + k).reduce((a, b) => a + b, 0));
      at += k;
    }
    const score = Math.abs(Math.log(w / (height / Math.max(1, tallest)) / o.aspect));
    if (score < bestScore - 1e-9) {
      bestScore = score;
      bestColumns = c;
    }
  }
  return bestColumns;
}

/** Lanes laid out automatically: same card order, columns from the window size. */
export function autoLanes(lanes: Lanes, of: ProjectOf, width: number, height: number, o: AutoOptions = AUTO): Lanes {
  const cards = groups(flatten(lanes), of);
  if (!cards.length) return [[]];
  const sizes = cards.map((c) => c.length);
  const counts = distribute(sizes, autoColumns(sizes, width, height, o));
  const out: Lanes = [];
  let at = 0;
  for (const k of counts) {
    out.push(cards.slice(at, at + k).flat());
    at += k;
  }
  return out;
}
