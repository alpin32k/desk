import { describe, expect, it } from "vitest";
import type { SessionStatus, Usage, UsageDetails } from "./api";
import { renderHistory, renderNow, renderPopover, renderSessions, type SessionView } from "./usage-view";

const now = Date.UTC(2026, 10, 14, 0, 55);
const s = now / 1000;
const H = 3600;

const fable: Usage = {
  agent: "claude",
  account: null,
  credits: false,
  resets: null,
  windows: [
    { label: "5h", usedPercent: 61, resetsAt: s + 6840, severity: "normal" },
    { label: "7d", usedPercent: 54, resetsAt: s + 5 * 86400, severity: "normal" },
  ],
  models: [{ name: "Fable", usedPercent: 95, resetsAt: s + 5 * 86400, severity: "critical", shareOfWeekly: 48, allowanceOfWeekly: 50 }],
};

const plain: Usage = { ...fable, credits: null, models: [] };

function status(slug: string, cost: number, more: Partial<SessionStatus> = {}): SessionStatus {
  return {
    slug,
    sessionId: `id-${slug}`,
    transcript: "",
    account: "main",
    model: "Opus 5.5",
    modelId: "claude-opus-5-5",
    effort: "high",
    fast: false,
    cost,
    contextPercent: 40,
    cacheWarm: true,
    cacheExpiresAt: s + 240,
    recacheTokens: 1000,
    ...more,
  };
}

function view(slug: string, cost: number, more: Partial<SessionStatus> = {}): SessionView {
  return { slug, key: `p/${slug}`, name: slug, state: "idle", status: status(slug, cost, more) };
}

const details: UsageDetails = { history: [], recordingSince: s - 3 * 86400, sessions: [], share: null };

function popover(u: Usage, layout: "tabs" | "all" = "tabs") {
  return renderPopover({ u, details, share: null, rows: [], layout, tab: "now", narrow: false, nowMs: now, title: "Limity planu · claude" });
}

describe("the Now view", () => {
  it("shows a model limit with its grading and what comes after it", () => {
    const html = renderNow(fable, now);
    expect(html).toContain("Fable");
    expect(html).toContain("95%");
    expect(html).toContain("kredyty wyłączone");
    expect(html).toContain('class="up-pct high"');
    expect(html).toContain("zużył 48% tygodnia, może najwyżej 50%");
  });

  it("has no model section without model limits", () => {
    const html = renderNow(plain, now);
    expect(html).not.toContain('data-section="models"');
    expect(html).not.toContain('data-section="resets"');
  });

  it("lists the resets on offer", () => {
    const u: Usage = {
      ...plain,
      resets: {
        fiveHour: { available: true, nextAvailableAt: null },
        saved: [{ label: "Prezent", left: 1, total: 2, endsAt: s + 10 * 86400, requiresLimit: true }],
      },
    };
    const html = renderNow(u, now);
    expect(html).toContain('data-section="resets"');
    expect(html).toContain("/limit-reset");
    expect(html).toContain("1 z 2");
    expect(html).toContain("działa dopiero przy limicie");
  });

  it("warns when the pace runs out before the reset", () => {
    const u: Usage = { ...plain, windows: [{ label: "5h", usedPercent: 50, resetsAt: s + 2 * H + 55 * 60, severity: null }] };
    const html = renderNow(u, now);
    expect(html).toContain("tempo 24%/h");
    expect(html).toContain("50 min przed resetem");
  });
});

describe("the popover", () => {
  it("has three tabs for Claude, with a dot on Now", () => {
    const html = popover(fable);
    expect(html.match(/data-tab="/g)).toHaveLength(3);
    expect(html).toMatch(/data-tab="now"[^>]*>[^<]*<i class="up-dot high">/);
  });

  it("has no sessions tab for Codex", () => {
    const html = popover({ ...plain, agent: "codex" });
    expect(html.match(/data-tab="/g)).toHaveLength(2);
    expect(html).not.toContain('data-tab="sessions"');
  });

  it("shows everything at once without tabs", () => {
    const html = popover(fable, "all");
    expect(html).not.toContain("data-tab=");
    expect(html).toContain("Fable");
    expect(html).toContain("Tydzień dzień po dniu");
  });
});

describe("the Sessions view", () => {
  it("puts closed sessions' share in a rest row", () => {
    const share = {
      mode: "cost" as const,
      incomplete: false,
      rows: [
        { slug: "a", label: "a", fraction: 0.6 },
        { slug: "closed", label: "closed", fraction: 0.4 },
      ],
    };
    const html = renderSessions([view("a", 2)], share, false, now);
    expect(html).toMatch(/reszta[\s\S]*40%/);
    expect(html).toContain('data-key="p/a"');
    expect(html).toContain("60%");
  });

  it("orders by share, then cost", () => {
    const share = { mode: "cost" as const, incomplete: true, rows: [{ slug: "b", label: "b", fraction: 1 }] };
    const html = renderSessions([view("a", 9), view("c", 1), view("b", 2)], share, true, now);
    expect(html.indexOf('data-key="p/b"')).toBeLessThan(html.indexOf('data-key="p/a"'));
    expect(html.indexOf('data-key="p/a"')).toBeLessThan(html.indexOf('data-key="p/c"'));
    expect(html).toContain("niepełne");
    expect(html).toContain("Kontekst");
  });

  it("says when the share is still being counted", () => {
    expect(renderSessions([view("a", 2)], "loading", false, now)).toContain("liczę udział");
  });

  it("warns about the largest cold cache", () => {
    const html = renderSessions(
      [view("a", 2, { cacheWarm: false, recacheTokens: 180_000 }), view("b", 1, { cacheWarm: false, recacheTokens: 60_000 })],
      null,
      false,
      now,
    );
    expect(html).toContain("zimny");
    expect(html).toMatch(/następna wiadomość w a wyśle od nowa ~180k tokenów/);
  });
});

describe("the History view", () => {
  it("says history is still being collected", () => {
    const html = renderHistory({ ...details, recordingSince: s - 2 * H }, plain, now);
    expect(html).toContain("desk zbiera historię od");
  });

  it("marks the week's days before desk recorded as without data", () => {
    const html = renderHistory({ ...details, recordingSince: s - 10 * H }, plain, now);
    expect(html).toContain('class="up-day none"');
  });

  it("draws 48 bars and the even share of the week", () => {
    const html = renderHistory(details, plain, now);
    expect(html.match(/class="up-b/g)).toHaveLength(48);
    expect(html).toContain("równo: 14% dziennie");
    expect(html).not.toContain("desk zbiera historię od");
  });
});
