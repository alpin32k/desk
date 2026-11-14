import { describe, expect, it } from "vitest";
import { pinAtlasCanvas } from "./atlas-font";

function fakes(isConnected: boolean) {
  const canvas = { isConnected, style: { display: "" } };
  const appended: unknown[] = [];
  const host = { append: (c: unknown) => appended.push(c) } as unknown as HTMLElement;
  const addon = { _renderer: { _charAtlas: { _tmpCanvas: canvas } } };
  return { canvas, appended, host, addon };
}

describe("pinAtlasCanvas", () => {
  it("moves a scratch canvas that is outside the document under the host, hidden", () => {
    const { canvas, appended, host, addon } = fakes(false);
    expect(pinAtlasCanvas(addon, host)).toBe(true);
    expect(appended).toEqual([canvas]);
    expect(canvas.style.display).toBe("none");
  });

  it("leaves a canvas that is already in the document where xterm put it", () => {
    const { appended, host, addon } = fakes(true);
    expect(pinAtlasCanvas(addon, host)).toBe(false);
    expect(appended).toEqual([]);
  });

  it("does nothing before the addon has an atlas", () => {
    const { appended, host } = fakes(false);
    expect(pinAtlasCanvas({}, host)).toBe(false);
    expect(pinAtlasCanvas({ _renderer: {} }, host)).toBe(false);
    expect(appended).toEqual([]);
  });
});
