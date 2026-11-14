import { beforeEach, describe, expect, it } from "vitest";
import type { FanStatus, ThermalStatus } from "./api";
import { setLanguage, t } from "./i18n";
import {
  clampFanLimit,
  clampLimit,
  modeOptions,
  slowing,
  thermalHtml,
  thermalLevel,
  thermalTitle,
  thermalToast,
} from "./thermal";

const at = (celsius: number | null, more: Partial<ThermalStatus> = {}): ThermalStatus => ({
  celsius,
  limit: 75,
  slowed: false,
  processes: 0,
  cannotSlow: null,
  fanLimit: 0,
  fans: null,
  ...more,
});

const fan = (more: Partial<FanStatus> = {}): FanStatus => ({ forced: false, rpm: 0, maxRpm: 6800, problem: null, ...more });
const forced = fan({ forced: true, rpm: 6800 });

describe("temperature chip", () => {
  beforeEach(() => setLanguage("pl"));

  it("shows degrees, yellow near the limit, red at it", () => {
    expect(thermalLevel(at(55))).toBe("");
    expect(thermalLevel(at(66))).toBe("warm");
    expect(thermalLevel(at(75))).toBe("hot");
    const html = thermalHtml(at(62));
    expect(html).toContain(">62°</button>");
    expect(html).toContain('class="top-btn thermal"');
    expect(html).toContain("Powyżej 75°C");
  });

  it("marks slowing with an arrow and says how many processes", () => {
    const s = at(82, { slowed: true, processes: 5 });
    expect(slowing(s)).toBe(true);
    const html = thermalHtml(s);
    expect(html).toContain(">82° ↓</button>");
    expect(html).toContain("thermal hot");
    expect(html).toContain("5 procesów");
    expect(html).toContain("68°C");
  });

  it("hot but nothing to slow, or slowing impossible: no arrow, the reason in the title", () => {
    expect(thermalHtml(at(82, { slowed: true }))).toContain(">82°</button>");
    const html = thermalHtml(at(82, { slowed: true, cannotSlow: "systemd (no bus)" }));
    expect(html).toContain("systemd (no bus)");
    expect(html).not.toContain("↓");
  });

  it("nothing without a sensor, unless the system says it is too hot", () => {
    expect(thermalHtml(null)).toBe("");
    expect(thermalHtml(at(null))).toBe("");
    expect(thermalHtml(at(null, { slowed: true, processes: 2 }))).toContain(">🌡 ↓</button>");
  });

  it("mode off: still shows the temperature, and says desk does nothing", () => {
    const html = thermalHtml(at(70, { limit: 0 }));
    expect(html).toContain(">70°</button>");
    expect(html).toContain("nic nie robi");
  });

  it("limits typed in Options stay sensible", () => {
    expect([10, 50, 72.6, 100, 300, NaN].map(clampLimit)).toEqual([50, 50, 73, 100, 100, 75]);
  });

  it("shows ≋ while the fans are forced", () => {
    expect(thermalHtml(at(78, { fanLimit: 70, fans: forced }))).toContain(">78° ≋</button>");
    expect(thermalHtml(at(81, { fanLimit: 70, fans: forced, slowed: true, processes: 3 }))).toContain(">81° ≋ ↓</button>");
  });

  it("is hot with forced fans and warm relative to the lowest limit", () => {
    expect(thermalLevel(at(61, { limit: 0, fanLimit: 70, fans: fan() }))).toBe("warm");
    expect(thermalLevel(at(50, { fanLimit: 70, fans: forced }))).toBe("hot");
  });

  it("titles name the rpm, the auto temperature and any problem", () => {
    const title = thermalTitle(at(78, { fanLimit: 70, fans: forced }));
    expect(title).toContain("6800");
    expect(title).toContain("63°C");
    expect(thermalTitle(at(78, { fanLimit: 70, fans: fan({ problem: "x-why" }) }))).toContain("x-why");
    const fansOnly = thermalTitle(at(60, { limit: 0, fanLimit: 75, fans: fan() }));
    expect(fansOnly.startsWith("Procesor: 60°C.")).toBe(true);
    expect(fansOnly).toContain("75°C");
    expect(thermalTitle(at(70, { limit: 0, fanLimit: 0 }))).toContain("nic nie robi");
  });

  it("limits typed in Options keep the fans at or below slowing", () => {
    expect([clampFanLimit(80, 75), clampFanLimit(10, 75), clampFanLimit(NaN, 75)]).toEqual([75, 50, 70]);
  });
});

describe("temperature toasts", () => {
  beforeEach(() => setLanguage("pl"));
  const cool = at(70, { fanLimit: 70, fans: fan() });
  const hotFans = at(72, { fanLimit: 70, fans: forced });
  const hotBoth = at(80, { fanLimit: 70, fans: forced, slowed: true, processes: 3 });

  it("fans on, fans off, and the two together", () => {
    expect(thermalToast(cool, hotFans)).toBe(t("thermal.fansOn", { c: 72 }));
    expect(thermalToast(cool, hotBoth)).toBe(t("thermal.slowingFans", { c: 80 }));
    expect(thermalToast(hotFans, at(62, { fanLimit: 70, fans: fan() }))).toBe(t("thermal.fansOff", { c: 62 }));
    expect(thermalToast(hotBoth, at(62, { fanLimit: 70, fans: fan() }))).toBe(t("thermal.cooledFans", { c: 62 }));
  });

  it("no toast when the fans stop because the mode dropped them", () => {
    expect(thermalToast(hotFans, at(72, { fanLimit: 0, fans: null }))).toBeNull();
  });

  it("a new problem once", () => {
    const stuck = at(72, { fanLimit: 70, fans: fan({ problem: "brak zgody" }) });
    expect(thermalToast(cool, stuck)).toBe(t("thermal.fansProblem", { why: "brak zgody" }));
    expect(thermalToast(stuck, stuck)).toBeNull();
  });

  it("slowing alone as before", () => {
    expect(thermalToast(at(70), at(80, { slowed: true, processes: 2 }))).toBe(t("thermal.slowing", { c: 80 }));
    expect(thermalToast(at(80, { slowed: true, processes: 2 }), at(66))).toBe(t("thermal.cooled", { c: 66 }));
  });
});

describe("modes in Options", () => {
  beforeEach(() => setLanguage("pl"));
  const why = (info: Parameters<typeof modeOptions>[0]) => modeOptions(info).map((o) => [o.mode, o.why]);

  it("fan modes are greyed out where desk cannot drive the fans", () => {
    expect(why({ unsupported: "u", consent: "missing" })).toEqual([
      ["off", null],
      ["slow", null],
      ["fansFirst", "u"],
      ["together", "u"],
      ["fans", "u"],
    ]);
    expect(why(null).filter(([, w]) => w === t("fans.unknown")).length).toBe(3);
    expect(why({ unsupported: null, consent: "installed" }).every(([, w]) => w === null)).toBe(true);
  });
});
