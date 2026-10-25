// Color themes, chosen in Options. Each theme is a terminal palette; the
// window's own colors (panels, lines, states) are derived from it.

import type { ITheme } from "@xterm/xterm";

export interface Theme {
  id: string;
  name: string;
  light?: boolean;
  bg: string;
  fg: string;
  cursor?: string;
  selection: string;
  /** black, red, green, yellow, blue, magenta, cyan, white, then the bright ones. */
  ansi: string[];
  /** Window colors where the derived ones do not fit. */
  ui?: Partial<Record<UiVar, string>>;
}

type UiVar = "bg" | "panel" | "line" | "line-hi" | "fg" | "dim" | "accent" | "working" | "waiting" | "idle" | "danger";

export const DEFAULT_THEME = "desk";

export const THEMES: Theme[] = [
  {
    id: "desk",
    name: "desk",
    bg: "#0b0d10",
    fg: "#c9d1d9",
    cursor: "#e6edf3",
    selection: "#264f78",
    ansi: [
      "#1b1f24", "#ff7b72", "#7ee787", "#e3b341", "#79c0ff", "#d2a8ff", "#56d4dd", "#c9d1d9",
      "#6e7681", "#ffa198", "#aff5b4", "#f8e3a1", "#a5d6ff", "#e2c5ff", "#b3f0ff", "#f0f6fc",
    ],
    ui: { panel: "#101318", line: "#1f242c", "line-hi": "#2d333b" },
  },
  {
    id: "cursor-dark",
    name: "Cursor Dark",
    bg: "#181818",
    fg: "#d6d6dd",
    selection: "#363636",
    ansi: [
      "#2a2a2a", "#f14c4c", "#15ac91", "#e5b95c", "#4c9df3", "#e567dc", "#75d3ba", "#d6d6dd",
      "#676767", "#f47070", "#3fc2a6", "#ebc88d", "#71b0f5", "#ec8ae4", "#9ae0cc", "#ffffff",
    ],
  },
  {
    id: "one-dark",
    name: "One Dark",
    bg: "#282c34",
    fg: "#abb2bf",
    selection: "#3e4451",
    ansi: [
      "#3f4451", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
      "#5c6370", "#e88388", "#a5d68c", "#ebd09c", "#7cbcf2", "#d499e5", "#6fcfd9", "#ffffff",
    ],
  },
  {
    id: "dracula",
    name: "Dracula",
    bg: "#282a36",
    fg: "#f8f8f2",
    selection: "#44475a",
    ansi: [
      "#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2",
      "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff",
    ],
  },
  {
    id: "monokai",
    name: "Monokai",
    bg: "#272822",
    fg: "#f8f8f2",
    selection: "#49483e",
    ansi: [
      "#3e3d32", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4", "#f8f8f2",
      "#75715e", "#ff5c8f", "#bdf35a", "#ffd494", "#8ae6f7", "#c4a3ff", "#c0f7ef", "#f9f8f5",
    ],
  },
  {
    id: "nord",
    name: "Nord",
    bg: "#2e3440",
    fg: "#d8dee9",
    selection: "#434c5e",
    ansi: [
      "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
      "#616e88", "#d08770", "#b5d1a0", "#f0d9a8", "#8fbcbb", "#c9a7c2", "#a3d3dd", "#eceff4",
    ],
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    bg: "#1a1b26",
    fg: "#c0caf5",
    selection: "#33467c",
    ansi: [
      "#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff", "#a9b1d6",
      "#565f89", "#ff899d", "#b3e083", "#f0c584", "#8db0ff", "#c7a9ff", "#a4daff", "#c0caf5",
    ],
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    bg: "#1e1e2e",
    fg: "#cdd6f4",
    selection: "#45475a",
    ansi: [
      "#45475a", "#f38ba8", "#a6e3a1", "#f9e2af", "#89b4fa", "#f5c2e7", "#94e2d5", "#bac2de",
      "#6c7086", "#f5a3b9", "#bbebb7", "#fbeac4", "#a3c4fb", "#f8d4ed", "#addcd4", "#a6adc8",
    ],
  },
  {
    id: "gruvbox-dark",
    name: "Gruvbox Dark",
    bg: "#282828",
    fg: "#ebdbb2",
    selection: "#504945",
    ansi: [
      "#3c3836", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#d5c4a1",
      "#928374", "#fc6d5c", "#cacd4f", "#fbcc5a", "#9db8ad", "#dea0b1", "#a7cf98", "#fbf1c7",
    ],
  },
  {
    id: "rose-pine",
    name: "Rosé Pine",
    bg: "#191724",
    fg: "#e0def4",
    selection: "#403d52",
    ansi: [
      "#26233a", "#eb6f92", "#31748f", "#f6c177", "#9ccfd8", "#c4a7e7", "#ebbcba", "#e0def4",
      "#6e6a86", "#f08ca8", "#4b8fa9", "#f8d09b", "#b5dbe2", "#d3bdee", "#f0cfcd", "#ffffff",
    ],
    ui: { working: "#9ccfd8" },
  },
  {
    id: "solarized-dark",
    name: "Solarized Dark",
    bg: "#002b36",
    fg: "#93a1a1",
    selection: "#073642",
    ansi: [
      "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5",
      "#586e75", "#cb4b16", "#9ab000", "#cc9f00", "#4ba3e0", "#6c71c4", "#3ec2b8", "#fdf6e3",
    ],
  },
  {
    id: "github-dark",
    name: "GitHub Dark",
    bg: "#0d1117",
    fg: "#e6edf3",
    selection: "#264f78",
    ansi: [
      "#484f58", "#ff7b72", "#3fb950", "#d29922", "#58a6ff", "#bc8cff", "#39c5cf", "#b1bac4",
      "#6e7681", "#ffa198", "#56d364", "#e3b341", "#79c0ff", "#d2a8ff", "#56d4dd", "#ffffff",
    ],
  },
  {
    id: "github-light",
    name: "GitHub Light",
    light: true,
    bg: "#ffffff",
    fg: "#1f2328",
    selection: "#b6e3ff",
    ansi: [
      "#24292f", "#cf222e", "#116329", "#9a6700", "#0969da", "#8250df", "#1b7c83", "#6e7781",
      "#57606a", "#a40e26", "#1a7f37", "#633c01", "#218bff", "#a475f9", "#3192aa", "#8c959f",
    ],
  },
  {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    light: true,
    bg: "#eff1f5",
    fg: "#4c4f69",
    selection: "#ccd0da",
    ansi: [
      "#5c5f77", "#d20f39", "#40a02b", "#df8e1d", "#1e66f5", "#ea76cb", "#179299", "#acb0be",
      "#6c6f85", "#de293e", "#49af3d", "#eea02d", "#456eff", "#fe85d8", "#2d9fa8", "#bcc0cc",
    ],
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    light: true,
    bg: "#fdf6e3",
    fg: "#586e75",
    selection: "#eee8d5",
    ansi: [
      "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#93a1a1",
      "#657b83", "#cb4b16", "#6f8000", "#9a7500", "#1f72ad", "#6c71c4", "#1f8a82", "#839496",
    ],
  },
  {
    id: "ayu-light",
    name: "Ayu Light",
    light: true,
    bg: "#fafafa",
    fg: "#5c6166",
    selection: "#d1e4f4",
    ansi: [
      "#000000", "#e65050", "#6cbf43", "#eba400", "#399ee6", "#a37acc", "#4cbf99", "#adaeb1",
      "#787b80", "#f07171", "#86b300", "#f2ae49", "#55b4d4", "#b38bd9", "#5ccfa5", "#c7c7c7",
    ],
  },
];

export function themeById(id: string | undefined): Theme {
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

/** `a` moved toward `b` by `t` (0..1). */
export function mix(a: string, b: string, t: number): string {
  const p = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const [x, y] = [p(a), p(b)];
  return `#${x.map((v, i) => Math.round(v + (y[i] - v) * t).toString(16).padStart(2, "0")).join("")}`;
}

export function terminalTheme(t: Theme): ITheme {
  const [black, red, green, yellow, blue, magenta, cyan, white] = t.ansi;
  const [brightBlack, brightRed, brightGreen, brightYellow, brightBlue, brightMagenta, brightCyan, brightWhite] =
    t.ansi.slice(8);
  return {
    background: t.bg,
    foreground: t.fg,
    cursor: t.cursor ?? t.fg,
    cursorAccent: t.bg,
    selectionBackground: t.selection,
    black,
    red,
    green,
    yellow,
    blue,
    magenta,
    cyan,
    white,
    brightBlack,
    brightRed,
    brightGreen,
    brightYellow,
    brightBlue,
    brightMagenta,
    brightCyan,
    brightWhite,
  };
}

/** The window's CSS colors for a theme. */
export function uiColors(t: Theme): Record<UiVar, string> {
  const dim = mix(t.fg, t.bg, t.light ? 0.4 : 0.45);
  return {
    bg: t.bg,
    panel: mix(t.bg, t.fg, t.light ? 0.035 : 0.03),
    line: mix(t.bg, t.fg, t.light ? 0.12 : 0.1),
    "line-hi": mix(t.bg, t.fg, t.light ? 0.22 : 0.18),
    fg: t.fg,
    dim,
    accent: t.ansi[4],
    working: t.ansi[2],
    waiting: t.ansi[3],
    idle: dim,
    danger: t.ansi[1],
    ...t.ui,
  };
}

export function applyTheme(t: Theme, root: HTMLElement = document.documentElement): void {
  for (const [k, v] of Object.entries(uiColors(t))) root.style.setProperty(`--${k}`, v);
  root.style.colorScheme = t.light ? "light" : "dark";
  root.dataset.theme = t.id;
}
