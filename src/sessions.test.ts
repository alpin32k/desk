import { describe, expect, it } from "vitest";
import { agentLabel, keyOf, nextInstance, parseKey } from "./sessions";

describe("session keys", () => {
  it("keeps the bare path for the main Claude session", () => {
    expect(keyOf({ path: "/a/b", agent: "claude", instance: 0 })).toBe("/a/b");
    expect(parseKey("/a/b")).toEqual({ path: "/a/b", agent: "claude", instance: 0 });
  });

  it("round-trips other agents and sub-sessions", () => {
    for (const s of [
      { path: "/a/b-c", agent: "codex", instance: 0 },
      { path: "/a/b", agent: "claude", instance: 2 },
      { path: "/x:y/z", agent: "my-agent", instance: 1 },
    ]) {
      expect(parseKey(keyOf(s))).toEqual(s);
    }
  });

  it("picks the next free sub-session number", () => {
    const taken = [
      { path: "/p", agent: "claude", instance: 0 },
      { path: "/p", agent: "claude", instance: 1 },
      { path: "/p", agent: "codex", instance: 0 },
      { path: "/q", agent: "claude", instance: 3 },
    ];
    expect(nextInstance(taken, "/p", "claude")).toBe(2);
    expect(nextInstance(taken, "/p", "codex")).toBe(1);
    expect(nextInstance(taken, "/p", "opencode")).toBe(0);
    expect(nextInstance([{ path: "/p", agent: "claude", instance: 1 }], "/p", "claude")).toBe(0);
  });

  it("labels sub-sessions for people", () => {
    expect(agentLabel({ path: "/p", agent: "claude", instance: 0 })).toBe("claude");
    expect(agentLabel({ path: "/p", agent: "claude", instance: 1 })).toBe("claude #2");
  });
});
