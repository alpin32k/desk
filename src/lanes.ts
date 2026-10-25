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
