import { describe, expect, it } from "vitest";
import { accountColor, accountFor, ACCOUNT_COLORS, hasAccounts, newAccountId, withAccount, withoutAccount } from "./accounts";
import type { ClaudeAccount } from "./api";

const main: ClaudeAccount = { id: "main", name: "Firma", dir: "" };
const own: ClaudeAccount = { id: "prywatne", name: "Prywatne", dir: "~/.claude-prywatne" };

describe("accounts", () => {
  it("shows accounts only with two or more", () => {
    expect(hasAccounts({ claudeAccounts: [] })).toBe(false);
    expect(hasAccounts({ claudeAccounts: [main] })).toBe(false);
    expect(hasAccounts({ claudeAccounts: [main, own] })).toBe(true);
  });

  it("a session keeps its account, a new one takes the project's, else the default", () => {
    const l = { claudeAccounts: [main, own], tileAccounts: { "/p": "prywatne" }, projectAccounts: { "/q": "prywatne" } };
    expect(accountFor(l, "/p", "/p")).toBe("prywatne");
    expect(accountFor(l, "claude#1:/q", "/q")).toBe("prywatne");
    expect(accountFor(l, "/r", "/r")).toBe("main");
    // An account removed in Options: back to the default.
    expect(accountFor({ ...l, tileAccounts: { "/p": "gone" } }, "/p", "/p")).toBe("main");
    expect(accountFor({ claudeAccounts: [], tileAccounts: {}, projectAccounts: {} }, "/p", "/p")).toBe("");
  });

  it("colors follow the place in the list", () => {
    const l = { claudeAccounts: [main, own] };
    expect(accountColor(l, "main")).toBe(ACCOUNT_COLORS[0]);
    expect(accountColor(l, "prywatne")).toBe(ACCOUNT_COLORS[1]);
    expect(accountColor(l, "gone")).toBe(ACCOUNT_COLORS[0]);
  });

  it("the first account added brings the default one along", () => {
    const list = withAccount([], "Prywatne", "", "Główne");
    expect(list).toEqual([
      { id: "main", name: "Główne", dir: "" },
      { id: "prywatne", name: "Prywatne", dir: "~/.claude-prywatne" },
    ]);
    const more = withAccount(list, "Żółć łąka", "/opt/c", "Główne");
    expect(more[2]).toEqual({ id: "zolc-laka", name: "Żółć łąka", dir: "/opt/c" });
    expect(newAccountId("Prywatne", list)).toBe("prywatne-2");
    expect(newAccountId("!!!", [])).toBe("konto");
  });

  it("removing an account forgets the tiles and projects set to it, never the default", () => {
    const l = { claudeAccounts: [main, own], tileAccounts: { "/p": "prywatne", "/q": "main" }, projectAccounts: { "/p": "prywatne" } };
    expect(withoutAccount(l, "prywatne")).toEqual({ claudeAccounts: [main], tileAccounts: { "/q": "main" }, projectAccounts: {} });
    expect(withoutAccount(l, "main")).toBe(l);
  });
});
