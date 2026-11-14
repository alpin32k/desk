// The usage chip's popover: opens under the chip, keeps its tab and scroll
// across refreshes, closes on Esc, a click elsewhere, the window losing focus
// or its chip going away. Content: usage-view.ts.
import type { Layout, SessionStatus, Usage, UsageDetails, UsageShare } from "./api";
import { chipId } from "./usage";
import { renderPopover, type SessionView } from "./usage-view";

export type Tab = "now" | "sessions" | "history";

export interface PopoverContext {
  usage(): Usage[];
  layout(): Layout;
  /** The desk sessions behind these status lines (by slug); ones without a tile are left out. */
  sessions(statuses: SessionStatus[]): SessionView[];
  details(agent: string, account: string | null, share: UsageShare | null): Promise<UsageDetails>;
  /** Focus a session's tile. */
  focus(key: string): void;
  /** HTML: "Limity planu · claude", with the account's name and color. */
  title(u: Usage): string;
}

/** The open chip, while a reading still shows it; else null. */
export function stillOpen(openId: string | null, usage: Usage[]): string | null {
  return openId !== null && usage.some((u) => u.windows.length && chipId(u) === openId) ? openId : null;
}

/** The share to ask for: only where it is shown (the sessions tab, or everything at once). */
export function shareRequest(layout: Layout, tab: Tab): UsageShare | null {
  return layout.usageLayout === "all" || tab === "sessions" ? layout.usageShare : null;
}

/** "All at once" needs this much room before it goes to one column. */
const ALL_WIDTH = 960 + 32;
/** The same details asked again after this long (Rust keeps them 30 s). */
const DETAILS_MS = 30_000;
const GAP = 8;

export class UsagePopover {
  private open: string | null = null;
  /** Kept until desk quits (spec: not in layout.json). */
  private tab: Tab = "now";
  private details: UsageDetails | null = null;
  /** What `details` answers: chip and share asked for. */
  private detailsFor: { id: string; share: UsageShare | null; at: number } | null = null;
  private pending = false;
  /** When the last request failed: asked again only after a while. */
  private failedAt = 0;
  private el: HTMLElement;
  /** What it shows now. */
  private html = "";

  constructor(
    host: HTMLElement,
    private ctx: PopoverContext,
  ) {
    this.el = document.createElement("div");
    this.el.className = "up-host";
    this.el.hidden = true;
    host.append(this.el);
    this.el.addEventListener("click", (e) => {
      const target = e.target as HTMLElement;
      const tab = target.closest<HTMLElement>("[data-tab]")?.dataset.tab as Tab | undefined;
      if (tab) {
        this.tab = tab;
        this.el.querySelector(".up-body")?.scrollTo(0, 0);
        this.refresh();
        return;
      }
      const key = target.closest<HTMLElement>("[data-key]")?.dataset.key;
      if (key) {
        this.close();
        this.ctx.focus(key);
      }
    });
    document.addEventListener(
      "mousedown",
      (e) => {
        const target = e.target as HTMLElement;
        if (this.open && !this.el.contains(target) && !target.closest("[data-usage]")) this.close();
      },
      true,
    );
    window.addEventListener(
      "keydown",
      (e) => {
        if (this.open && e.key === "Escape") {
          e.preventDefault();
          e.stopImmediatePropagation();
          this.close();
        }
      },
      true,
    );
    window.addEventListener("blur", () => this.close());
    window.addEventListener("resize", () => this.refresh());
  }

  get openId(): string | null {
    return this.open;
  }

  toggle(id: string): void {
    if (this.open === id) return this.close();
    this.open = id;
    this.details = null;
    this.detailsFor = null;
    this.refresh();
  }

  close(): void {
    if (!this.open) return;
    this.open = null;
    this.el.hidden = true;
    this.el.innerHTML = this.html = "";
    this.chips().forEach((c) => c.removeAttribute("aria-expanded"));
  }

  /** Draws it again from the latest readings, keeping its tab and scroll; closes when its chip is gone. */
  refresh(): void {
    const id = stillOpen(this.open, this.ctx.usage());
    const chip = id ? this.chips().find((c) => c.dataset.usage === id) : undefined;
    const u = this.ctx.usage().find((x) => chipId(x) === id);
    if (!id || !chip || !u) return this.close();
    for (const c of this.chips()) c.setAttribute("aria-expanded", String(c === chip));
    const layout = this.ctx.layout();
    const share = shareRequest(layout, this.tab);
    this.fetch(id, u, share);
    const fresh = this.details && this.detailsFor?.id === id ? this.details : null;
    const html = renderPopover({
      u,
      details: fresh,
      share: share === null ? null : fresh && this.detailsFor?.share === share ? fresh.share : "loading",
      rows: fresh ? this.ctx.sessions(fresh.sessions) : [],
      layout: layout.usageLayout,
      tab: this.tab,
      narrow: window.innerWidth < ALL_WIDTH,
      nowMs: Date.now(),
      title: this.ctx.title(u),
    });
    // The bar redraws often: rebuild only when something shows differently
    // (a click between mousedown and mouseup would otherwise be lost).
    if (html !== this.html || this.el.hidden) {
      const scroll = this.el.querySelector(".up-body")?.scrollTop ?? 0;
      this.el.innerHTML = this.html = html;
      this.el.hidden = false;
      const body = this.el.querySelector(".up-body");
      if (body) body.scrollTop = scroll;
    }
    this.place(chip);
  }

  /**
   * One request at a time; again when the chip or the share changed or after
   * 30 s. Details with a share also serve a view without one.
   */
  private fetch(id: string, u: Usage, share: UsageShare | null): void {
    const asked = this.detailsFor;
    const answered = asked && asked.id === id && (share === null || asked.share === share);
    const now = Date.now();
    if (this.pending || now - this.failedAt < 10_000 || (answered && now - asked.at < DETAILS_MS)) return;
    this.pending = true;
    this.ctx
      .details(u.agent, u.account, share)
      .then((d) => {
        if (this.open !== id) return;
        this.details = d;
        this.detailsFor = { id, share, at: Date.now() };
      })
      .catch(() => {
        // Shown as still loading; asked again a little later.
        this.failedAt = Date.now();
      })
      .finally(() => {
        this.pending = false;
        if (this.open) this.refresh();
      });
  }

  /** Under the chip, its right edge on the chip's, inside the window; the nib points at the chip. */
  private place(chip: HTMLElement): void {
    const r = chip.getBoundingClientRect();
    const box = this.el.firstElementChild as HTMLElement | null;
    const width = box?.offsetWidth ?? 0;
    const left = Math.max(GAP, Math.min(r.right - width, window.innerWidth - width - GAP));
    const top = r.bottom + GAP;
    this.el.style.left = `${left}px`;
    this.el.style.top = `${top}px`;
    this.el.style.maxHeight = `${window.innerHeight - top - GAP}px`;
    const nib = Math.max(r.left + r.width / 2, r.right - 24);
    box?.style.setProperty("--nib", `${Math.max(12, left + width - nib - 5)}px`);
  }

  private chips(): HTMLElement[] {
    return [...document.querySelectorAll<HTMLElement>("[data-usage]")];
  }
}
