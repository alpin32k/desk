// One font for every glyph of a WebGL terminal.
//
// xterm rasterizes glyphs on a scratch canvas. It pre-draws ASCII 33-125 in idle
// time while that canvas is still outside the document, and draws everything else
// on demand after moving it under the terminal. WebKit resolves a font installed
// in ~/Library/Fonts only for a canvas inside the document, so with
// "JetBrainsMono Nerd Font Mono" installed the pre-drawn ASCII came out in SF Mono
// and the on-demand glyphs (ą, ł, ó, colored text) in JetBrains Mono: thinner and
// 3 px higher in the same line. Putting the scratch canvas into the document
// before the pre-draw runs makes both passes resolve the same font.

/** What this reads from the addon: private fields of @xterm/addon-webgl 0.19. */
interface AtlasOwner {
  _renderer?: { _charAtlas?: { _tmpCanvas?: HTMLCanvasElement } };
}

/**
 * Put the addon's scratch canvas under `host` (hidden) when it is outside the
 * document. Returns whether it moved it. A no-op when the addon has no atlas yet
 * or a newer xterm keeps the canvas somewhere else.
 */
export function pinAtlasCanvas(addon: object, host: HTMLElement): boolean {
  const canvas = (addon as AtlasOwner)._renderer?._charAtlas?._tmpCanvas;
  if (!canvas || canvas.isConnected) return false;
  canvas.style.display = "none";
  host.append(canvas);
  return true;
}
