import { describe, expect, it } from "vitest";
import type { GithubRepo } from "./api";
import { matchRepos, repoName } from "./github";

const repo = (fullName: string): GithubRepo => ({ fullName, description: null, private: false });
const names = (list: GithubRepo[]) => list.map((r) => r.fullName);

describe("matchRepos", () => {
  const repos = ["acme/crm", "me/desk", "knehrybecki/desk", "acme/desk-site", "me/notes"].map(repo);

  it("puts name prefixes first and keeps GitHub's order otherwise", () => {
    expect(names(matchRepos(repos, "desk"))).toEqual(["me/desk", "knehrybecki/desk", "acme/desk-site"]);
    expect(names(matchRepos(repos, "site"))).toEqual(["acme/desk-site"]);
  });

  it("matches the owner too, every word, ignoring case", () => {
    expect(names(matchRepos(repos, "ACME"))).toEqual(["acme/crm", "acme/desk-site"]);
    expect(names(matchRepos(repos, "me desk"))).toEqual(["me/desk", "acme/desk-site"]);
  });

  it("shows nothing for an empty query and respects the limit", () => {
    expect(matchRepos(repos, "  ")).toEqual([]);
    expect(matchRepos(repos, "e", 2)).toHaveLength(2);
  });
});

describe("repoName", () => {
  it("drops the owner", () => {
    expect(repoName(repo("knehrybecki/desk"))).toBe("desk");
  });
});
