import { afterEach, describe, expect, it } from "vitest";
import { detectLanguage, setLanguage, t, tn } from "./i18n";
import { en } from "./locales/en";
import { pl } from "./locales/pl";
import { clashWith, hints } from "./keys";
import { sessionsLabel } from "./sessions";

const placeholders = (text: string) => [...new Set([...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]))].sort();

describe("i18n", () => {
  afterEach(() => setLanguage("pl"));

  it("fills every message's placeholders in both languages", () => {
    for (const key of Object.keys(pl) as (keyof typeof pl)[]) {
      expect(placeholders(en[key]), key).toEqual(placeholders(pl[key]));
      expect(en[key].split("|").length > 1, key).toBe(pl[key].split("|").length > 1);
    }
  });

  it("takes the first preferred language desk speaks, else English", () => {
    expect(detectLanguage(["pl-PL", "en-US"])).toBe("pl");
    expect(detectLanguage(["en-GB", "pl"])).toBe("en");
    expect(detectLanguage(["de-DE", "pl_PL"])).toBe("pl");
    expect(detectLanguage(["de-DE", "fr"])).toBe("en");
    expect(detectLanguage([])).toBe("en");
  });

  it("fills placeholders and leaves unknown ones", () => {
    expect(t("app.sessionEnded", { name: "desk" })).toBe("Sesja desk zakończona");
    expect(t("app.sessionEnded")).toBe("Sesja {name} zakończona");
  });

  it("switches the language of messages, plurals and shortcut labels", () => {
    setLanguage("en");
    expect(t("app.sessionEnded", { name: "desk" })).toBe("Session desk ended");
    expect([1, 2, 5, 22].map(sessionsLabel)).toEqual(["1 session", "2 sessions", "5 sessions", "22 sessions"]);
    expect(tn("side.items", 0)).toBe("0 items");
    expect(hints("mac")[0]).toEqual(["⌘P", "projects"]);
    expect(clashWith("palette", "Super+KeyC", "mac")).toBe("copy");
  });
});
