import { describe, expect, it } from "vitest";
import type { Conversation } from "./api";
import { conversationRows, matchesRow, setSaved, syncTile } from "./saved";

const A = "0f096eef-0a11-4750-93d4-7f09562dc824";
const B = "18f4a809-bf1a-49c3-b7a3-ccbc13fbcd9f";
const C = "e873f5be-20e1-435c-b644-bacf67a3fb73";

const conv = (id: string, modifiedMs: number, title: string | null = null, prompt = "hej", branch: string | null = "main"): Conversation => ({
  id,
  title,
  prompt,
  branch,
  modifiedMs,
});

describe("saving a conversation under a name", () => {
  it("saves, renames and forgets per project", () => {
    let saved = setSaved({}, "/crm", A, "SSO wdrożenie");
    saved = setSaved(saved, "/desk", A, "Inny projekt");
    expect(saved).toEqual({ "/crm": { [A]: "SSO wdrożenie" }, "/desk": { [A]: "Inny projekt" } });
    saved = setSaved(saved, "/crm", A, "  SSO  ");
    expect(saved["/crm"]).toEqual({ [A]: "SSO" });
    // An empty name forgets it, and a project with nothing saved goes.
    saved = setSaved(saved, "/crm", A, " ");
    expect(saved).toEqual({ "/desk": { [A]: "Inny projekt" } });
  });

  it("does not change the map it was given", () => {
    const before = { "/crm": { [A]: "SSO" } };
    setSaved(before, "/crm", B, "Tickety");
    expect(before).toEqual({ "/crm": { [A]: "SSO" } });
  });
});

describe("a named tile and the conversation in it", () => {
  it("saves the conversation of a tile named before it was saved", () => {
    expect(syncTile({}, "/crm", "SSO", A)).toEqual({ save: "SSO" });
  });

  it("does nothing while they agree, or without a conversation or a name", () => {
    const saved = { "/crm": { [A]: "SSO" } };
    expect(syncTile(saved, "/crm", "SSO", A)).toEqual({});
    expect(syncTile(saved, "/crm", "SSO", null)).toEqual({});
    expect(syncTile(saved, "/crm", null, B)).toEqual({});
  });

  it("leaves the name with the old conversation after /clear", () => {
    expect(syncTile({ "/crm": { [A]: "SSO" } }, "/crm", "SSO", B)).toEqual({ title: null });
    // Another project's conversation of that name does not count.
    expect(syncTile({ "/desk": { [A]: "SSO" } }, "/crm", "SSO", B)).toEqual({ save: "SSO" });
  });

  it("takes the name of a saved conversation picked up in the tile", () => {
    const saved = { "/crm": { [A]: "SSO", [B]: "Tickety" } };
    expect(syncTile(saved, "/crm", "SSO", B)).toEqual({ title: "Tickety" });
    expect(syncTile(saved, "/crm", null, A)).toEqual({ title: "SSO" });
  });
});

describe("⌘S rows", () => {
  const found = [conv(C, 30, "Webhook"), conv(B, 20, null, "napraw tickety"), conv(A, 10, "Przywrócenie sesji")];

  it("puts the saved ones first, newest first, then the rest", () => {
    const rows = conversationRows(found, { [A]: "SSO wdrożenie", [B]: "Tickety" });
    expect(rows.map((r) => [r.c.id, r.name])).toEqual([
      [B, "Tickety"],
      [A, "SSO wdrożenie"],
      [C, null],
    ]);
  });

  it("filters by the name, Claude's title, the first message and the branch", () => {
    const rows = conversationRows(found, { [A]: "SSO wdrożenie" });
    const hits = (q: string) => rows.filter((r) => matchesRow(r, q)).map((r) => r.c.id);
    expect(hits("sso")).toEqual([A]);
    expect(hits("SSO przywr")).toEqual([A]);
    expect(hits("tickety")).toEqual([B]);
    expect(hits("main")).toEqual([A, C, B]);
    expect(hits("")).toHaveLength(3);
    expect(hits("nic")).toEqual([]);
  });
});
