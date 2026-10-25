// Pure operations on lanes: columns of project paths, top to bottom.
export type Lanes = string[][];

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

export function add(lanes: Lanes, path: string): Lanes {
  if (contains(lanes, path)) return lanes;
  const next = clone(lanes.length ? lanes : [[]]);
  next[shortestLane(next)].push(path);
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
