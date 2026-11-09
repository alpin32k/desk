import { beforeEach, describe, expect, it } from "vitest";
import type { ThermalStatus } from "./api";
import { setLanguage } from "./i18n";
import { clampLimit, slowing, thermalHtml, thermalLevel } from "./thermal";

const at = (celsius: number | null, more: Partial<ThermalStatus> = {}): ThermalStatus => ({
  celsius,
  limit: 75,
  slowed: false,
  processes: 0,
  cannotSlow: null,
  ...more,
});

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

  it("limit off: still shows the temperature, never red for the limit", () => {
    const html = thermalHtml(at(70, { limit: 0 }));
    expect(html).toContain(">70°</button>");
    expect(html).toContain("wyłączone");
  });

  it("limits typed in Options stay sensible", () => {
    expect([10, 50, 72.6, 100, 300, NaN].map(clampLimit)).toEqual([50, 50, 73, 100, 100, 75]);
  });
});
