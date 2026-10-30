// Pointer dragging in the grid: the lines between lanes, cards and tiles, and
// moving a card or a tile. Pointer events rather than HTML5 drag and drop: the
// window takes every file drop (app.ts), and the drop mark follows the pointer.
import { split } from "./lanes";
import { holdFits } from "./tile";

/** The weight an element grows by, read from and written to the layout. */
export interface Sizer {
  get(): number;
  set(weight: number): void;
}

const sizers = new WeakMap<Element, Sizer>();
/** Narrowest lane or tile side by side, lowest card or tile, in CSS px. */
const MIN = { v: 160, h: 48 };

/** Let `el` take room by its weight and be resized by the lines next to it. */
export function sizable(el: HTMLElement, sizer: Sizer): void {
  sizers.set(el, sizer);
  el.style.flexGrow = String(sizer.get());
}

/**
 * A line between two sizable siblings ("v" between columns, "h" between
 * rows): dragging it moves room from one to the other, a double click shares
 * it evenly. `done` runs once it is let go.
 */
export function splitLine(dir: "v" | "h", done: () => void): HTMLElement {
  const line = document.createElement("div");
  line.className = `split ${dir}`;
  const pos = (e: PointerEvent) => (dir === "v" ? e.clientX : e.clientY);
  const length = (el: Element) => (dir === "v" ? el.getBoundingClientRect().width : el.getBoundingClientRect().height);
  line.addEventListener("pointerdown", (e) => {
    const a = neighbour(line, -1);
    const b = neighbour(line, 1);
    if (e.button !== 0 || !a || !b) return;
    e.preventDefault();
    e.stopPropagation();
    const [sa, sb] = [sizers.get(a)!, sizers.get(b)!];
    const [pa, pb, total, start] = [length(a), length(b), sa.get() + sb.get(), pos(e)];
    line.setPointerCapture(e.pointerId);
    line.classList.add("on");
    holdFits(true);
    let moved = false;
    const move = (ev: PointerEvent) => {
      moved = true;
      const [wa, wb] = split(total, pa, pb, pos(ev) - start, MIN[dir]);
      sa.set(wa);
      sb.set(wb);
      a.style.flexGrow = String(wa);
      b.style.flexGrow = String(wb);
    };
    const up = () => {
      line.removeEventListener("pointermove", move);
      line.removeEventListener("pointerup", up);
      line.removeEventListener("pointercancel", up);
      line.classList.remove("on");
      holdFits(false);
      // A plain click changes nothing; laying out again would also lose a double click.
      if (moved) done();
    };
    line.addEventListener("pointermove", move);
    line.addEventListener("pointerup", up);
    line.addEventListener("pointercancel", up);
  });
  line.addEventListener("dblclick", () => {
    const a = neighbour(line, -1);
    const b = neighbour(line, 1);
    if (!a || !b) return;
    const [sa, sb] = [sizers.get(a)!, sizers.get(b)!];
    const half = (sa.get() + sb.get()) / 2;
    sa.set(half);
    sb.set(half);
    done();
  });
  return line;
}

/** The nearest sibling the line can resize: sizable, shown and not folded. */
function neighbour(line: Element, step: -1 | 1): HTMLElement | null {
  let el = step < 0 ? line.previousElementSibling : line.nextElementSibling;
  while (el) {
    if (el instanceof HTMLElement && sizers.has(el) && el.getClientRects().length && getComputedStyle(el).flexGrow !== "0") return el;
    el = step < 0 ? el.previousElementSibling : el.nextElementSibling;
  }
  return null;
}

/**
 * Follow a press that may become a drag: past a few pixels `start` runs, then
 * `move` on every move, and `drop` at the end (null when cancelled). A drag
 * swallows the click that follows it.
 */
export function watchDrag(
  e: PointerEvent,
  h: { start(): void; move(ev: PointerEvent): void; drop(ev: PointerEvent | null): void },
): void {
  if (e.button !== 0) return;
  const [x, y] = [e.clientX, e.clientY];
  let started = false;
  const move = (ev: PointerEvent) => {
    if (!started) {
      if (Math.hypot(ev.clientX - x, ev.clientY - y) < 5) return;
      started = true;
      document.documentElement.classList.add("dragging-card");
      h.start();
    }
    ev.preventDefault();
    h.move(ev);
  };
  const end = (ev: PointerEvent) => {
    window.removeEventListener("pointermove", move, true);
    window.removeEventListener("pointerup", end, true);
    window.removeEventListener("pointercancel", end, true);
    if (!started) return;
    document.documentElement.classList.remove("dragging-card");
    mark(null);
    label(null);
    const swallow = (c: Event) => c.stopPropagation();
    window.addEventListener("click", swallow, true);
    setTimeout(() => window.removeEventListener("click", swallow, true));
    h.drop(ev.type === "pointerup" ? ev : null);
  };
  window.addEventListener("pointermove", move, true);
  window.addEventListener("pointerup", end, true);
  window.addEventListener("pointercancel", end, true);
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

let markEl: HTMLElement | null = null;
let labelEl: HTMLElement | null = null;

/** Where a drop lands: a line, or a whole area (`zone`); null hides it. */
export function mark(box: Box | null, zone = false): void {
  if (!box) return markEl?.remove();
  markEl ??= document.createElement("div");
  markEl.className = zone ? "drop-mark zone" : "drop-mark";
  Object.assign(markEl.style, { left: `${box.x}px`, top: `${box.y}px`, width: `${box.w}px`, height: `${box.h}px` });
  if (!markEl.isConnected) document.body.appendChild(markEl);
}

/** A label next to the pointer saying what is dragged and where; null hides it. */
export function label(text: string | null, ev?: PointerEvent): void {
  if (text === null || !ev) return labelEl?.remove();
  labelEl ??= document.createElement("div");
  labelEl.className = "drag-label";
  labelEl.textContent = text;
  labelEl.style.left = `${ev.clientX + 14}px`;
  labelEl.style.top = `${ev.clientY + 12}px`;
  if (!labelEl.isConnected) document.body.appendChild(labelEl);
}
